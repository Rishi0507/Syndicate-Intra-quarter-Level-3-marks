package soi

import (
	"archive/zip"
	"bufio"
	"fmt"
	"io"
	"strconv"
	"strings"

	"syndicate/ingest/internal/day"
)

type Row struct {
	Adsh   string
	CIK    string
	Holder string
	Form   string
	AsOf   day.Date
	Filed  day.Date
	Period day.Date

	Sector      string
	Identifier  string
	Affiliation string
	Type        string
	Issuer      string
	Entity      string
	Currency    string
	RateType    string
	Shares      float64

	Rate      float64
	Spread    float64
	Maturity  day.Date
	Principal float64
	Cost      float64
	FairValue float64

	HasRate, HasSpread, HasPrincipal, HasCost, HasFairValue bool
}

type columns struct {
	adsh, cik, name, form, ddate, filed, period int
	sector, ident, affil, typ                   int
	issuer, entity, currency, shares, rateType  int
	rate, spread, maturity, principal           int
	cost, fair                                  []int
}

func squash(s string) string {
	var b strings.Builder
	for _, r := range strings.ToLower(s) {
		if r >= 'a' && r <= 'z' {
			b.WriteRune(r)
		}
	}
	return b.String()
}

func locate(header []string) (columns, error) {
	var c columns
	idx := map[string]int{}
	for i, h := range header {
		k := squash(h)
		if _, dup := idx[k]; !dup {
			idx[k] = i
		}
	}
	find := func(names ...string) int {
		for _, n := range names {
			if i, ok := idx[n]; ok {
				return i
			}
		}
		return -1
	}
	c.adsh = find("adsh")
	c.cik = find("cik")
	c.name = find("name")
	c.form = find("form")
	c.ddate = find("ddate")
	c.filed = find("filed")
	c.period = find("period")
	c.sector = find("industrysectoraxis")
	c.ident = find("investmentidentifieraxis")
	c.affil = find("investmentissueraffiliationaxis")
	c.typ = find("investmenttypeaxis")
	c.rate = find("investmentinterestrate")
	c.spread = find("investmentbasisspreadvariablerate")
	c.maturity = find("investmentmaturitydate")
	c.principal = find("investmentownedbalanceprincipalamount")
	c.issuer = find("investmentissuernameaxis")
	c.entity = find("legalentityaxis")
	c.currency = find("currencyaxis")
	c.rateType = find("investmentvariableinterestratetypeextensibleenumeration")
	c.shares = find("investmentshares", "investmentownedbalanceshares")
	all := func(names ...string) []int {
		var out []int
		for _, n := range names {
			if i, ok := idx[n]; ok {
				out = append(out, i)
			}
		}
		return out
	}
	c.cost = all("investmentownedcost", "investmentownedatcost", "adjustedcostbasis")
	c.fair = all("investmentownedfairvalue", "investmentownedatfairvalue", "initialfairvalueofinvestment")

	var missing []string
	for _, r := range []struct {
		name string
		pos  int
	}{{"cik", c.cik}, {"name", c.name}, {"ddate", c.ddate}, {"filed", c.filed},
		{"Investment, Identifier Axis", c.ident}} {
		if r.pos < 0 {
			missing = append(missing, r.name)
		}
	}
	if len(c.fair) == 0 {
		missing = append(missing, "a fair value column")
	}
	if len(missing) > 0 {
		return c, fmt.Errorf("soi header is missing required columns: %s", strings.Join(missing, "; "))
	}
	return c, nil
}

func field(parts []string, i int) string {
	if i < 0 || i >= len(parts) {
		return ""
	}
	return strings.TrimSpace(parts[i])
}

func firstNumber(parts []string, candidates []int) (float64, bool) {
	for _, i := range candidates {
		if v, ok := number(parts, i); ok {
			return v, true
		}
	}
	return 0, false
}

func number(parts []string, i int) (float64, bool) {
	s := field(parts, i)
	if s == "" {
		return 0, false
	}
	v, err := strconv.ParseFloat(strings.ReplaceAll(s, ",", ""), 64)
	if err != nil {
		return 0, false
	}
	return v, true
}

type Stats struct {
	Lines   int
	Rows    int
	Skipped int
}

func Read(r io.Reader, fn func(*Row) error) (Stats, error) {
	var st Stats
	br := bufio.NewReaderSize(r, 1<<20)

	line, err := readLine(br)
	if err == io.EOF && strings.TrimSpace(line) == "" {
		return st, nil
	}
	if err != nil && err != io.EOF {
		return st, fmt.Errorf("reading soi header: %w", err)
	}
	line = strings.TrimPrefix(line, "\xef\xbb\xbf")
	cols, err := locate(strings.Split(line, "\t"))
	if err != nil {
		return st, err
	}

	var row Row
	for {
		line, err = readLine(br)
		if err == io.EOF && line == "" {
			break
		}
		if err != nil && err != io.EOF {
			return st, err
		}
		st.Lines++
		parts := strings.Split(line, "\t")
		if len(parts) <= cols.ident || len(parts) <= cols.ddate {
			st.Skipped++
		} else {
			row = Row{
				Adsh:        field(parts, cols.adsh),
				CIK:         strings.TrimLeft(field(parts, cols.cik), "0"),
				Holder:      field(parts, cols.name),
				Form:        field(parts, cols.form),
				AsOf:        day.Parse(field(parts, cols.ddate)),
				Filed:       day.Parse(field(parts, cols.filed)),
				Period:      day.Parse(field(parts, cols.period)),
				Sector:      field(parts, cols.sector),
				Identifier:  field(parts, cols.ident),
				Affiliation: field(parts, cols.affil),
				Type:        field(parts, cols.typ),
				Issuer:      field(parts, cols.issuer),
				Entity:      field(parts, cols.entity),
				Currency:    field(parts, cols.currency),
				RateType:    field(parts, cols.rateType),
				Maturity:    day.Parse(field(parts, cols.maturity)),
			}
			row.Shares, _ = number(parts, cols.shares)
			row.Rate, row.HasRate = number(parts, cols.rate)
			row.Spread, row.HasSpread = number(parts, cols.spread)
			row.Principal, row.HasPrincipal = number(parts, cols.principal)
			row.Cost, row.HasCost = firstNumber(parts, cols.cost)
			row.FairValue, row.HasFairValue = firstNumber(parts, cols.fair)
			st.Rows++
			if e := fn(&row); e != nil {
				return st, e
			}
		}
		if err == io.EOF {
			break
		}
	}
	return st, nil
}

func readLine(br *bufio.Reader) (string, error) {
	var sb strings.Builder
	for {
		chunk, isPrefix, err := br.ReadLine()
		if err != nil {
			return sb.String(), err
		}
		sb.Write(chunk)
		if !isPrefix {
			return sb.String(), nil
		}
	}
}

func ReadZip(path string, fn func(*Row) error) (Stats, error) {
	zr, err := zip.OpenReader(path)
	if err != nil {
		return Stats{}, err
	}
	defer zr.Close()

	for _, f := range zr.File {
		base := strings.ToLower(f.Name)
		if i := strings.LastIndexAny(base, "/\\"); i >= 0 {
			base = base[i+1:]
		}
		if base != "soi.tsv" {
			continue
		}
		rc, err := f.Open()
		if err != nil {
			return Stats{}, err
		}
		st, err := Read(rc, fn)
		rc.Close()
		return st, err
	}
	return Stats{}, fmt.Errorf("%s: no soi.tsv in archive", path)
}
