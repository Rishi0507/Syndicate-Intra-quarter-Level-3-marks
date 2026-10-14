package xbrl

import (
	"encoding/xml"
	"fmt"
	"io"
	"sort"
	"strings"
	"unicode"
)

const (
	nsInstance  = "http://www.xbrl.org/2003/instance"
	nsDimension = "http://xbrl.org/2006/xbrldi"
	nsLink      = "http://www.xbrl.org/2003/linkbase"
	nsXlink     = "http://www.w3.org/1999/xlink"
	nsSchemaIn  = "http://www.w3.org/2001/XMLSchema-instance"
	nsXML       = "http://www.w3.org/XML/1998/namespace"
	roleLabel   = "http://www.xbrl.org/2003/role/label"
)

var Axes = []string{
	"IndustrySectorAxis",
	"InvestmentIdentifierAxis",
	"InvestmentIssuerAffiliationAxis",
	"InvestmentTypeAxis",
	"InvestmentIssuerNameAxis",
	"LegalEntityAxis",
	"CurrencyAxis",
}

var Values = []string{
	"InvestmentInterestRate",
	"InvestmentBasisSpreadVariableRate",
	"InvestmentMaturityDate",
	"InvestmentOwnedBalancePrincipalAmount",
	"InvestmentOwnedAtCost",
	"InvestmentOwnedAtFairValue",
	"InvestmentOwnedBalanceShares",
	"InvestmentOwnedPercentOfNetAssets",
	"InvestmentInterestRatePaidInKind",
	"InvestmentVariableInterestRateTypeExtensibleEnumeration",
}

func Header() []string {
	h := []string{"adsh", "cik", "name", "ddate", "form", "filed", "period"}
	h = append(h, Axes...)
	h = append(h, Values...)
	return append(h, "OtherAxes")
}

type Filing struct {
	Adsh   string
	CIK    string
	Name   string
	Form   string
	Filed  string
	Period string
}

type member struct {
	axis  string
	value string
	typed bool
}

type context struct {
	date string
	dims []member
}

type Instance struct {
	contexts map[string]*context
	facts    map[string]map[string]string
}

var wanted = func() map[string]bool {
	m := map[string]bool{}
	for _, v := range Values {
		m[v] = true
	}
	return m
}()

var axisColumn = func() map[string]int {
	m := map[string]int{}
	for i, a := range Axes {
		m[a] = i
	}
	return m
}()

func local(qname string) string {
	if i := strings.LastIndexByte(qname, ':'); i >= 0 {
		return qname[i+1:]
	}
	return qname
}

func attr(e xml.StartElement, space, name string) string {
	for _, a := range e.Attr {
		if a.Name.Local == name && (space == "" || a.Name.Space == space) {
			return a.Value
		}
	}
	return ""
}

func charset(label string, r io.Reader) (io.Reader, error) {
	switch strings.ToLower(strings.TrimSpace(label)) {
	case "us-ascii", "ascii", "utf-8", "utf8":
		return r, nil
	case "iso-8859-1", "latin1", "latin-1", "windows-1252", "cp1252":
		b, err := io.ReadAll(r)
		if err != nil {
			return nil, err
		}
		rs := make([]rune, len(b))
		for i, c := range b {
			rs[i] = rune(c)
		}
		return strings.NewReader(string(rs)), nil
	}
	return nil, fmt.Errorf("unsupported encoding %q", label)
}

func decoder(r io.Reader) *xml.Decoder {
	dec := xml.NewDecoder(r)
	dec.Strict = false
	dec.CharsetReader = charset
	return dec
}

func ParseInstance(r io.Reader) (*Instance, error) {
	in := &Instance{contexts: map[string]*context{}, facts: map[string]map[string]string{}}
	dec := decoder(r)

	var (
		ctx     *context
		inTyped bool
		capture bool
		text    strings.Builder
		assign  func(string)
	)
	start := func(f func(string)) {
		capture, assign = true, f
		text.Reset()
	}
	for {
		tok, err := dec.Token()
		if err == io.EOF {
			break
		}
		if err != nil {
			return nil, fmt.Errorf("instance: %w", err)
		}
		switch t := tok.(type) {
		case xml.StartElement:
			switch {
			case t.Name.Space == nsInstance && t.Name.Local == "context":
				ctx = &context{}
				in.contexts[attr(t, "", "id")] = ctx
			case ctx == nil:
				if !wanted[t.Name.Local] || !strings.Contains(t.Name.Space, "us-gaap") {
					continue
				}
				ref, name := attr(t, "", "contextRef"), t.Name.Local
				if ref == "" || attr(t, nsSchemaIn, "nil") == "true" {
					continue
				}
				start(func(v string) {
					f := in.facts[ref]
					if f == nil {
						f = map[string]string{}
						in.facts[ref] = f
					}
					if _, dup := f[name]; !dup {
						f[name] = v
					}
				})
			case t.Name.Space == nsInstance && (t.Name.Local == "instant" || t.Name.Local == "endDate"):
				c := ctx
				start(func(v string) { c.date = v })
			case t.Name.Space == nsDimension && t.Name.Local == "explicitMember":
				c, axis := ctx, local(attr(t, "", "dimension"))
				start(func(v string) { c.dims = append(c.dims, member{axis: axis, value: v}) })
			case t.Name.Space == nsDimension && t.Name.Local == "typedMember":
				inTyped = true
				c, axis := ctx, local(attr(t, "", "dimension"))
				start(func(v string) { c.dims = append(c.dims, member{axis: axis, value: v, typed: true}) })
			case inTyped:
				text.Reset()
			}
		case xml.CharData:
			if capture {
				text.Write(t)
			}
		case xml.EndElement:
			switch {
			case t.Name.Space == nsInstance && t.Name.Local == "context":
				ctx = nil
				continue
			case t.Name.Space == nsDimension && t.Name.Local == "typedMember":
				inTyped = false
			case inTyped:
				continue
			}
			if capture {
				assign(strings.TrimSpace(text.String()))
				capture, assign = false, nil
			}
		}
	}
	return in, nil
}

func ParseLabels(r io.Reader) (map[string]string, error) {
	out := map[string]string{}
	dec := decoder(r)

	type label struct {
		role, lang, text string
	}
	var (
		locs   map[string]string
		labels map[string][]label
		arcs   [][2]string
		cur    *label
		key    string
		text   strings.Builder
	)
	reset := func() {
		locs, labels, arcs = map[string]string{}, map[string][]label{}, nil
	}
	resolve := func() {
		for _, a := range arcs {
			el, ok := locs[a[0]]
			if !ok {
				continue
			}
			for _, l := range labels[a[1]] {
				if l.role != roleLabel {
					continue
				}
				if _, have := out[el]; !have || strings.HasPrefix(strings.ToLower(l.lang), "en-us") {
					out[el] = l.text
				}
			}
		}
		reset()
	}
	reset()
	for {
		tok, err := dec.Token()
		if err == io.EOF {
			break
		}
		if err != nil {
			return nil, fmt.Errorf("labels: %w", err)
		}
		switch t := tok.(type) {
		case xml.StartElement:
			if t.Name.Space != nsLink {
				continue
			}
			switch t.Name.Local {
			case "loc":
				href := attr(t, nsXlink, "href")
				if i := strings.IndexByte(href, '#'); i >= 0 {
					locs[attr(t, nsXlink, "label")] = href[i+1:]
				}
			case "label":
				cur, key = &label{role: attr(t, nsXlink, "role"), lang: attr(t, nsXML, "lang")}, attr(t, nsXlink, "label")
				text.Reset()
			case "labelArc":
				arcs = append(arcs, [2]string{attr(t, nsXlink, "from"), attr(t, nsXlink, "to")})
			}
		case xml.CharData:
			if cur != nil {
				text.Write(t)
			}
		case xml.EndElement:
			if t.Name.Space != nsLink {
				continue
			}
			switch t.Name.Local {
			case "label":
				if cur != nil {
					cur.text = strings.Join(strings.Fields(text.String()), " ")
					labels[key] = append(labels[key], *cur)
					cur = nil
				}
			case "labelLink":
				resolve()
			}
		}
	}
	resolve()
	return out, nil
}

func humanize(name string) string {
	name = strings.TrimSuffix(name, "Member")
	var b strings.Builder
	rs := []rune(name)
	for i, r := range rs {
		if i > 0 && unicode.IsUpper(r) && (unicode.IsLower(rs[i-1]) || (i+1 < len(rs) && unicode.IsLower(rs[i+1]) && unicode.IsUpper(rs[i-1]))) {
			b.WriteByte(' ')
		}
		b.WriteRune(r)
	}
	return b.String()
}

func memberLabel(qname string, labels map[string]string) string {
	id := strings.Replace(qname, ":", "_", 1)
	if l, ok := labels[id]; ok && l != "" {
		return l
	}
	return humanize(local(qname))
}

func clean(s string) string {
	return strings.Map(func(r rune) rune {
		if r == '\t' || r == '\n' || r == '\r' {
			return ' '
		}
		return r
	}, s)
}

type Row []string

func (in *Instance) Rows(f Filing, labels map[string]string) []Row {
	ids := make([]string, 0, len(in.facts))
	for id := range in.facts {
		ids = append(ids, id)
	}
	sort.Strings(ids)

	var rows []Row
	for _, id := range ids {
		c := in.contexts[id]
		if c == nil || c.date == "" {
			continue
		}
		axes := make([]string, len(Axes))
		var other []string
		for _, m := range c.dims {
			v := m.value
			if !m.typed {
				v = memberLabel(m.value, labels)
			}
			if i, ok := axisColumn[m.axis]; ok && axes[i] == "" {
				axes[i] = v
			} else {
				other = append(other, m.axis+"="+v)
			}
		}
		if axes[axisColumn["InvestmentIdentifierAxis"]] == "" {
			continue
		}
		sort.Strings(other)
		facts := in.facts[id]
		row := Row{f.Adsh, f.CIK, f.Name, c.date, f.Form, f.Filed, f.Period}
		row = append(row, axes...)
		for _, v := range Values {
			row = append(row, facts[v])
		}
		row = append(row, strings.Join(other, "; "))
		for i := range row {
			row[i] = clean(row[i])
		}
		rows = append(rows, row)
	}
	return rows
}
