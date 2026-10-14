package ident

import (
	"regexp"
	"strings"
)

type Lien uint8

const (
	LienUnknown Lien = iota
	FirstLien
	Unitranche
	SecondLien
	Subordinated
	Equity
	Structured
)

var lienNames = [...]string{"unknown", "first_lien", "unitranche", "second_lien", "subordinated", "equity", "structured"}

func (l Lien) String() string { return lienNames[l] }

func (l Lien) IsDebt() bool { return l != Equity && l != Structured }

type Facility uint8

const (
	Term Facility = iota
	Revolver
	DelayedDraw
)

var facilityNames = [...]string{"term", "revolver", "delayed_draw"}

func (f Facility) String() string { return facilityNames[f] }

type Parsed struct {
	Borrower   string
	Aliases    []string
	Descriptor string
	Lien       Lien
	Facility   Facility
	Industry   string
	Unfunded   bool
}

func (p Parsed) Nameless() bool {
	if len(words(p.Borrower)) == 0 || isBoilerplate(p.Borrower) || isIndustry(p.Borrower) {
		return true
	}
	if lower := strings.ToLower(p.Borrower); strings.Contains(lower, "net assets") || strings.Contains(lower, "%") {
		return true
	}
	for _, w := range words(p.Borrower) {
		if !boilerplate[w] && !instrumentWords[w] && !isDigits(w) {
			return false
		}
	}
	return true
}

var instrumentWords = map[string]bool{
	"revolver": true, "revolving": true, "delayed": true, "draw": true, "tranche": true, "facility": true,
	"note": true, "bond": true, "bonds": true, "units": true, "shares": true, "class": true, "series": true,
	"interest": true, "membership": true, "partnership": true, "lp": true, "llc": true, "a": true, "b": true,
	"c": true, "incremental": true, "initial": true, "closing": true, "date": true, "priority": true,
	"super": true, "stop": true, "one": true, "growth": true, "capital": true, "equipment": true,
	"financing": true, "convertible": true, "promissory": true, "pik": true, "cash": true,
	"maturity": true, "acquisition": true, "services": true, "debt": true, "investment": true,
	"investments": true, "inc": true, "corp": true, "ltd": true, "co": true, "company": true,
	"holdings": true, "and": true,
}

var (
	acquiredRe     = regexp.MustCompile(`(?i)\s+(?:acquisition date|investment\s?type|maturity date|interest rate).*$`)
	spaceRe        = regexp.MustCompile(`\s+`)
	footRe         = regexp.MustCompile(`\s*(\(\s*\d{1,2}\s*\)|\(\s*[a-z]\s*\)|\[\s*\d{1,2}\s*\])`)
	aliasRe        = regexp.MustCompile(`(?i)\(\s*(?:d/?b/?a|f/?k/?a|a/?k/?a|formerly known as|formerly|also known as)\.?:?\s+([^)]+)\)`)
	parenRe        = regexp.MustCompile(`\(([^()]*)\)`)
	termRe         = regexp.MustCompile(`(?i)\d\s*%|\d{1,2}/\d{2,4}|\b[sl]\s*\+\s*\d|\bsofr\b|\blibor\b|\bprime\b|\beuribor\b|\bsonia\b`)
	indexRe        = regexp.MustCompile(`\s+(\d{1,2}|20\d\d)$`)
	spelledRe      = regexp.MustCompile(`(?i)\s+(one|two|three|four|five|six|seven|eight|nine|ten)$`)
	headingRe      = regexp.MustCompile(`[-\x{2013}\x{2014}]\s*\d+(?:\.\d+)?\s*%\s*[-\x{2013}\x{2014}]?\s+`)
	unfundedRe     = regexp.MustCompile(`(?i)\bcommitment (type|expiration)\b|\bunfunded\b|\bundrawn\b`)
	gluedRe        = regexp.MustCompile(`([A-Za-z.,)])(Type of Investment|Investment Type|Asset Type|Maturity Date|Interest Rate|Reference Rate)`)
	runOnHeadingRe = regexp.MustCompile(`(?i)(investments?)(non[-\s]*control)`)
	midHeadingRe   = regexp.MustCompile(`(?i)\b(?:non[-\s]*)?control(?:led)?\s*/\s*(?:non[-\s]*)?affiliat\w*\s+(?:investments?|issuers?|portfolio\s+compan(?:y|ies))\b[:\s]*`)
	inlineAliasRe  = regexp.MustCompile(`(?i)\s+(?:d/b/a|dba|f/k/a|fka)\s+(.+)$`)
	trailingRe     = regexp.MustCompile(`(?i)\s+(?:incremental|initial|replacement|extended|refinancing|add-on|amended|closing date|super priority|priority|tranche\s+[a-z0-9-]{1,3})$`)
	openParenRe    = regexp.MustCompile(`\s*\([^)]*$`)
	pipeRe         = regexp.MustCompile(`\s*\|\s*`)
	dashRe         = regexp.MustCompile(`\s+[\x{2013}\x{2014}-]\s+`)
)

var legalSuffix = map[string]bool{
	"inc": true, "inc.": true, "incorporated": true, "llc": true, "l.l.c.": true,
	"lp": true, "l.p.": true, "llp": true, "ltd": true, "ltd.": true, "limited": true,
	"corp": true, "corp.": true, "corporation": true, "co": true, "co.": true,
	"plc": true, "gmbh": true, "s.a.": true, "sa": true, "b.v.": true, "bv": true,
	"n.v.": true, "nv": true, "ag": true, "pty": true, "ulc": true, "s.a.r.l.": true,
	"sarl": true, "s.à r.l.": true, "s.a r.l.": true, "pllc": true, "p.c.": true, "lllp": true,
}

var companyWords = map[string]bool{
	"inc": true, "llc": true, "lp": true, "llp": true, "ltd": true, "limited": true, "corp": true,
	"corporation": true, "co": true, "company": true, "plc": true, "gmbh": true, "sarl": true, "bv": true,
	"nv": true, "ag": true, "pty": true, "ulc": true, "holdings": true, "holdco": true, "bidco": true,
	"topco": true, "midco": true, "parent": true, "buyer": true, "purchaser": true, "borrower": true,
	"intermediate": true, "merger": true, "group": true, "partners": true,
}

var strong = []string{
	"first lien", "first-lien", "1st lien", "second lien", "second-lien", "2nd lien",
	"third lien", "first out", "last out", "last-out", "senior secured", "senior unsecured",
	"senior subordinated", "junior secured", "secured debt", "secured loan",
	"secured note", "term loan", "delayed draw", "delay draw", "revolving",
	"revolver", "unitranche", "one stop", "subordinated", "mezzanine",
	"preferred equity", "preferred stock", "preferred units", "preferred shares",
	"common equity", "common stock", "common units", "common shares",
	"member units", "membership interest", "lp interest", "llc interest",
	"partnership interest", "limited partner interest", "class a", "class b", "warrants", "warrant to",
	"joint venture", "growth capital loan", "equity co-investment", "equity securities",
	"structured finance", "collateralized loan", "collaterized loan",
}

var labels = []string{
	"asset type", "investment type", "type of investment", "instrument", "commitment type",
	"industry", "reference rate", "interest rate", "interest term", "maturity",
	"initial acquisition", "investment date", "current coupon", "basis point", "total coupon",
	"effective yield", "sofr", "libor", "euribor", "sonia",
}

var investmentNext = map[string]bool{
	"first": true, "second": true, "senior": true, "junior": true, "unitranche": true,
	"subordinated": true, "preferred": true, "common": true, "equity": true, "warrants": true,
	"secured": true, "unsecured": true, "1st": true, "2nd": true, "mezzanine": true,
}

var weak = []string{
	"loan", "loans", "note", "notes", "bond", "bonds", "debenture", "debt",
	"preferred", "common", "equity", "units", "unit", "warrant", "shares",
	"stock", "interest", "interests", "tranche", "facility", "incremental",
	"series", "class", "prime", "floor", "pik", "due", "maturity", "matures",
	"ddtl", "unsecured", "secured", "senior", "junior", "lien", "affiliated", "issuer",
}

var boilerplate = map[string]bool{
	"investment": true, "investments": true, "portfolio": true, "company": true, "companies": true,
	"non": true, "control": true, "controlled": true, "affiliate": true, "affiliated": true,
	"affiliates": true, "unaffiliated": true, "issuer": true, "issuers": true, "funded": true,
	"unfunded": true, "drawn": true, "undrawn": true, "debt": true, "equity": true, "securities": true,
	"corporate": true, "us": true, "u": true, "s": true, "united": true, "states": true, "canadian": true,
	"european": true, "bank": true, "loans": true, "loan": true, "first": true, "second": true,
	"1st": true, "2nd": true, "lien": true, "senior": true, "secured": true, "unsecured": true,
	"junior": true, "subordinated": true, "and": true, "other": true, "in": true, "of": true, "the": true,
	"last": true, "out": true, "unitranche": true, "mezzanine": true, "notes": true, "term": true,
	"preferred": true, "common": true, "stock": true, "warrants": true, "total": true, "type": true,
	"largest": true, "smallest": true, "average": true, "all": true, "others": true, "various": true,
}

var prefixRes = compileAll(
	`^[-\x{2013}\x{2014}:,;/.]+\s*`,
	`^\d+(?:\.\d+)?\s*%\s*`,
	`^\(\s*\d{1,2}\s*\)\s*`,
	`^i?nvestments?\b\s*(?:in\b)?`,
	`^p?ortfolio\s+(?:compan(?:y|ies)|investments?)\b`,
	`^type\b`,
	`^(?:n?on[-\s]*)?control(?:led)?\b\s*[/,]?\s*(?:(?:non[-\s]*)?affiliat\w*)?(?:\s+(?:issuers?|investments?|portfolio\s+compan(?:y|ies)))*`,
	`^(?:n?on[-\s]*)?affiliat\w*(?:\s+(?:issuers?|investments?))*`,
	`^(?:d?ebt|e?quity|funded)\s+(?:and\s+equity\s+)?(?:investments?|securities|instruments)\b`,
	`^(?:1st|2nd|first|second)\s+lien(?:\s*/\s*(?:senior\s+secured\s+debt|last[-\s]out\s+unitranche))?(?:\s+(?:senior\s+)?(?:secured\s+)?(?:debt|loans?))?(?:\s+investments?)?\b`,
	`^(?:senior\s+)?(?:secured|unsecured|subordinated)\s+(?:debt|loans?|notes?)(?:\s+investments?)?\b`,
	`^(?:common|preferred)\s+(?:equity|stock)(?:\s*/\s*warrants)?\b`,
	`^(?:structured\s+finance\s+obligation|equity\s+and\s+other|unitranche|mezzanine)\b`,
	`^bank\s+loans\b`,
	`^(?:us|u\.s\.|canadian|european)\s+corporate\s+debt\b`,
)

var countryRe = regexp.MustCompile(`(?i)^(?:united\s+states|united\s+kingdom|australia|canada|cayman\s+islands|luxembourg|germany|france|netherlands|ireland|sweden|europe|israel|singapore|bermuda|jersey|switzerland|belgium|spain|italy|denmark|norway|finland|new\s+zealand)\b`)

var headingStart = regexp.MustCompile(`(?i)^(?:i?nvestments?\b|p?ortfolio\s+(?:compan|invest)|(?:n?on[-\s]*)?control|(?:n?on[-\s]*)?affiliat|(?:d?ebt|e?quity|funded)\s+(?:and\s+equity\s+)?(?:investments?|securities)|bank\s+loans|type\s+(?:non[-\s]*)?control)`)

func compileAll(patterns ...string) []*regexp.Regexp {
	out := make([]*regexp.Regexp, len(patterns))
	for i, p := range patterns {
		out[i] = regexp.MustCompile(`(?i)` + p)
	}
	return out
}

func hasWord(s, w string) bool {
	for i := 0; ; {
		j := strings.Index(s[i:], w)
		if j < 0 {
			return false
		}
		j += i
		before := j == 0 || !isAlnum(s[j-1])
		after := j+len(w) == len(s) || !isAlnum(s[j+len(w)])
		if before && after {
			return true
		}
		i = j + 1
	}
}

func isAlnum(b byte) bool {
	return b >= 'a' && b <= 'z' || b >= '0' && b <= '9'
}

func isBoilerplate(seg string) bool {
	w := words(seg)
	if len(w) == 0 {
		return true
	}
	for _, x := range w {
		if !boilerplate[x] && !isDigits(x) {
			return false
		}
	}
	return true
}

func hasCompanyWord(seg string) bool {
	for _, x := range words(seg) {
		if companyWords[x] {
			return true
		}
	}
	return false
}

func descriptorLike(seg string) bool {
	l := strings.ToLower(strings.TrimSpace(seg))
	if l == "" {
		return true
	}
	if legalSuffix[l] {
		return false
	}
	for _, p := range strong {
		if strings.Contains(l, p) {
			return true
		}
	}
	for _, w := range weak {
		if hasWord(l, w) {
			return true
		}
	}
	return termRe.MatchString(l)
}

func nameEnd(s string) int {
	l := strings.ToLower(s)
	best := len(s)
	consider := func(phrase string, plural bool) {
		for from := 0; ; {
			i := strings.Index(l[from:], phrase)
			if i < 0 {
				return
			}
			i += from
			tail := l[i+len(phrase):]
			start := i == 0 || !isAlnum(l[i-1])
			end := tail == "" || !isAlnum(tail[0]) || (plural && tail[0] == 's' && (len(tail) == 1 || !isAlnum(tail[1])))
			if start && end && i >= 2 {
				if i < best {
					best = i
				}
				return
			}
			from = i + 1
		}
	}
	for _, p := range strong {
		consider(p, true)
	}
	for _, p := range labels {
		consider(p, false)
	}
	for from := 0; ; {
		i := strings.Index(l[from:], " acquisition ")
		if i < 0 {
			break
		}
		i += from
		rest := strings.TrimLeft(l[i+len(" acquisition "):], " ")
		if i >= 2 && rest != "" && rest[0] >= '0' && rest[0] <= '9' {
			if i < best {
				best = i
			}
			break
		}
		from = i + 1
	}
	for from := 0; ; {
		i := strings.Index(l[from:], " investment ")
		if i < 0 {
			break
		}
		i += from
		next := strings.Fields(l[i+len(" investment "):])
		if i >= 2 && len(next) > 0 && investmentNext[strings.Trim(next[0], ",.-/")] {
			if i < best {
				best = i
			}
			break
		}
		from = i + 1
	}
	if m := dashRe.FindStringIndex(s); m != nil && m[0] >= 2 && m[0] < best {
		best = m[0]
	}
	return best
}

func stripHeadings(s, holder string) (rest, industry string) {
	if m := headingRe.FindAllStringIndex(s, -1); m != nil {
		s = s[m[len(m)-1][1]:]
	}
	if m := midHeadingRe.FindAllStringIndex(s, -1); m != nil && m[len(m)-1][1] < len(s) {
		s = s[m[len(m)-1][1]:]
	}
	stripped := false
	for changed := true; changed; {
		changed = false
		s = strings.TrimSpace(s)
		for _, re := range prefixRes {
			if m := re.FindStringIndex(s); m != nil && m[1] > 0 && m[1] < len(s) {
				s = s[m[1]:]
				changed, stripped = true, true
				break
			}
		}
		if changed {
			continue
		}
		if hw := words(holder); len(hw) > 0 {
			sw := words(s)
			same := len(sw) > len(hw)
			for i := 0; same && i < len(hw); i++ {
				same = sw[i] == hw[i]
			}
			if same {
				ends := wordEnds(s, len(hw))
				s = s[ends[len(ends)-1]:]
				changed, stripped = true, true
				continue
			}
		}
		if stripped {
			if m := countryRe.FindStringIndex(s); m != nil && m[1] < len(s) {
				s = s[m[1]:]
				changed = true
			}
		}
	}
	if rest, ok := stripIndustry(s); ok {
		industry = strings.TrimSpace(strings.TrimSuffix(strings.TrimSpace(s), rest))
		s = rest
		for changed := true; changed; {
			changed = false
			for _, re := range prefixRes {
				if m := re.FindStringIndex(s); m != nil && m[1] > 0 && m[1] < len(s) {
					s = strings.TrimSpace(s[m[1]:])
					changed = true
					break
				}
			}
		}
	}
	return strings.TrimSpace(s), strings.Trim(industry, " ,:-")
}

func dedupe(name string) string {
	f := strings.Fields(name)
	for k := len(f) / 2; k >= 1; k-- {
		same := true
		for i := 0; i < k; i++ {
			if !strings.EqualFold(strings.Trim(f[i], ",."), strings.Trim(f[k+i], ",.")) {
				same = false
				break
			}
		}
		if same {
			return strings.Join(f[k:], " ")
		}
	}
	return name
}

const debtHeading = "debt investments"

func TrimLabels(name string) string {
	return strings.TrimSpace(acquiredRe.ReplaceAllString(name, ""))
}

func tidy(name string, p *Parsed) string {
	name = strings.TrimLeft(name, "?*• 	")
	if i := strings.LastIndex(strings.ToLower(name), debtHeading); i >= 0 {
		name = strings.TrimSpace(name[i+len(debtHeading):])
		name = strings.TrimSpace(strings.TrimPrefix(name, "United States"))
		if rest, ok := stripIndustry(name); ok {
			name = rest
		}
	}
	name = acquiredRe.ReplaceAllString(name, "")
	for _, m := range aliasRe.FindAllStringSubmatch(name, -1) {
		if a := strings.TrimSpace(m[1]); a != "" {
			p.Aliases = append(p.Aliases, a)
		}
	}
	name = aliasRe.ReplaceAllString(name, "")
	name = footRe.ReplaceAllString(name, "")
	if m := inlineAliasRe.FindStringSubmatch(name); m != nil {
		p.Aliases = append(p.Aliases, strings.TrimSpace(m[1]))
		name = inlineAliasRe.ReplaceAllString(name, "")
	}
	for _, m := range parenRe.FindAllStringSubmatch(name, -1) {
		inner := strings.TrimSpace(m[1])
		if inner != "" && !descriptorLike(inner) && !isDigits(inner) {
			p.Aliases = append(p.Aliases, inner)
		}
	}
	name = parenRe.ReplaceAllString(name, "")
	name = openParenRe.ReplaceAllString(name, "")
	name = spaceRe.ReplaceAllString(strings.TrimSpace(name), " ")
	name = strings.TrimRight(name, ",-:;/( ")
	for {
		before := name
		if strings.Count(name, " ") >= 1 {
			name = indexRe.ReplaceAllString(name, "")
			name = spelledRe.ReplaceAllString(name, "")
			name = trailingRe.ReplaceAllString(name, "")
		}
		name = strings.TrimRight(name, ",-:;/( ")
		if name == before {
			break
		}
	}
	return dedupe(name)
}

func Parse(identifier, typeAxis string) Parsed {
	return ParseFor(identifier, typeAxis, "")
}

func ParseFor(identifier, typeAxis, holder string) Parsed {
	s := spaceRe.ReplaceAllString(strings.TrimSpace(identifier), " ")
	s = strings.TrimSpace(strings.Trim(s, `"'`))

	var p Parsed
	p.Unfunded = unfundedRe.MatchString(s)
	for _, m := range aliasRe.FindAllStringSubmatch(s, -1) {
		if a := strings.TrimSpace(m[1]); a != "" {
			p.Aliases = append(p.Aliases, a)
		}
	}
	s = spaceRe.ReplaceAllString(aliasRe.ReplaceAllString(s, ""), " ")
	s = strings.ReplaceAll(s, " ,", ",")
	s = gluedRe.ReplaceAllString(s, "$1 $2")
	s = runOnHeadingRe.ReplaceAllString(s, "$1 $2")
	name := ""

	switch {
	case strings.Contains(s, "|"):
		name, p.Industry = fromSegments(pipeRe.Split(s, -1), holder)
	case strings.Contains(strings.ToLower(s), "issuer name "):
		i := strings.Index(strings.ToLower(s), "issuer name ") + len("issuer name ")
		rest := s[i:]
		name = rest[:nameEnd(rest)]
	case headingStart.MatchString(s) || headingRe.MatchString(s) || midHeadingRe.MatchString(s) || hasLabel(s):
		var rest string
		rest, p.Industry = stripHeadings(s, holder)
		name = rest[:nameEnd(rest)]
	default:
		name, p.Industry = fromCommas(s)
	}

	p.Borrower = tidy(name, &p)
	p.Descriptor = s
	if p.Borrower != "" {
		if i := strings.Index(s, p.Borrower); i >= 0 {
			p.Descriptor = strings.TrimSpace(s[:i] + " " + s[i+len(p.Borrower):])
		}
	}
	p.Lien, p.Facility = Classify(typeAxis, p.Descriptor)
	lower := strings.ToLower(p.Borrower)
	if hasWord(lower, "clo") || hasWord(lower, "jv") || strings.Contains(lower, "senior loan program") ||
		strings.Contains(lower, "senior loan fund") || strings.Contains(lower, "joint venture") {
		p.Lien = Structured
	}
	return p
}

func hasLabel(s string) bool {
	l := strings.ToLower(s)
	for _, x := range []string{" asset type ", " investment type ", " type of investment ", " instrument ", " reference rate", " maturity date ", " interest rate "} {
		if strings.Contains(l, x) {
			return true
		}
	}
	return false
}

func fromSegments(segs []string, holder string) (name, industry string) {
	pick, picked, settled := "", false, false
	for _, seg := range segs {
		seg = strings.TrimSpace(seg)
		if seg == "" || isBoilerplate(seg) {
			continue
		}
		if isIndustry(seg) {
			if industry == "" {
				industry = indexRe.ReplaceAllString(seg, "")
			}
			continue
		}
		if settled {
			continue
		}
		head := seg
		if headingStart.MatchString(seg) {
			head, _ = stripHeadings(seg, holder)
		}
		head = strings.TrimSpace(head[:nameEnd(head)])
		if head == "" || isBoilerplate(head) || isIndustry(head) || termRe.MatchString(head) {
			continue
		}
		if !picked {
			pick, picked = head, true
			settled = hasCompanyWord(head)
			continue
		}
		if hasCompanyWord(head) {
			pick, settled = head, true
		}
	}
	return pick, industry
}

func fromCommas(s string) (name, industry string) {
	segs := strings.Split(s, ",")
	for i := range segs {
		segs[i] = strings.TrimSpace(segs[i])
	}
	suffix := func(seg string) bool {
		return legalSuffix[strings.ToLower(indexRe.ReplaceAllString(seg, ""))]
	}
	company := func(i int) bool {
		return hasCompanyWord(segs[i]) || (i+1 < len(segs) && suffix(segs[i+1]))
	}
	notName := func(i int) bool {
		seg := segs[i]
		return isBoilerplate(seg) || isIndustry(seg) || countryRe.MatchString(seg) ||
			strings.HasPrefix(strings.ToLower(seg), "united states") || Parsed{Borrower: seg}.Nameless()
	}
	first := 0
	if notName(0) {
		for i := 1; i < len(segs) && i <= 3; i++ {
			if !notName(i) && company(i) {
				first = i
				break
			}
		}
	}
	for _, seg := range segs[first+1:] {
		if isIndustry(seg) {
			industry = seg
			break
		}
	}
	end := first + 1
	for end < len(segs) && suffix(segs[end]) {
		end++
	}
	for j := end; j < len(segs) && j <= end+1; j++ {
		if suffix(segs[j]) && !descriptorLike(segs[end]) {
			end = j + 1
			break
		}
	}
	name = strings.Join(segs[first:end], ", ")
	if end == first+1 {
		name = name[:nameEnd(name)]
	}
	return name, industry
}

func Classify(typeAxis, descriptor string) (Lien, Facility) {
	t := strings.ToLower(camelSplit(typeAxis))
	d := strings.ToLower(descriptor)

	lien := lienOf(t)
	if lien == LienUnknown {
		lien = lienOf(d)
	}

	both := t + " " + d
	fac := Term
	switch {
	case strings.Contains(both, "revolv"):
		fac = Revolver
	case strings.Contains(both, "delayed draw"), strings.Contains(both, "delay draw"), hasWord(both, "ddtl"):
		fac = DelayedDraw
	}
	return lien, fac
}

func lienOf(s string) Lien {
	if s == "" {
		return LienUnknown
	}
	has := func(subs ...string) bool {
		for _, x := range subs {
			if strings.Contains(s, x) {
				return true
			}
		}
		return false
	}
	switch {
	case hasWord(s, "clo"), has("collateralized loan", "collaterized loan", "loan obligation", "structured", "joint venture",
		"investment fund", "senior loan fund", "private fund", "money market", "cash equivalent", "derivative"):
		return Structured
	case has("second lien", "second-lien", "2nd lien", "junior secured", "junior lien", "junior loan", "third lien"):
		return SecondLien
	case has("unitranche", "one stop", "last out", "last-out", "first out"):
		return Unitranche
	case has("subordinated", "mezzanine", "unsecured"):
		return Subordinated
	case has("first lien", "first-lien", "1st lien", "senior secured", "secured debt", "secured loan", "secured note", "senior loan"):
		return FirstLien
	case has("junior"):
		return Subordinated
	case has("equity", "common", "preferred", "warrant", "units", "shares", "stock",
		"membership", "partnership interest", "lp interest", "llc interest", "partner interest"):
		return Equity
	}
	return LienUnknown
}

func camelSplit(s string) string {
	var b strings.Builder
	for i, r := range s {
		if i > 0 && r >= 'A' && r <= 'Z' {
			prev := rune(s[i-1])
			if prev >= 'a' && prev <= 'z' {
				b.WriteByte(' ')
			}
		}
		b.WriteRune(r)
	}
	return b.String()
}
