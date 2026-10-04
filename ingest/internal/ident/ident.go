// Package ident turns the free text a BDC puts in the Investment Identifier
// axis into structure: who the borrower is, where the claim sits in the
// capital structure, and what kind of facility it is.
//
// There is no shared grammar. Reading two hundred registrants' schedules
// turns up four families, and the parser handles each:
//
//	Ares:        Acme Holdings, Inc., First lien senior secured loan
//	Golub:       Acme Holdings, LLC | One stop 1 | Non-Affiliated Issuer
//	Blackstone:  Acme Holdings, Inc. 2 | Non-Affiliated Issuer
//	Antares:     Investments - non-controlled/non-affiliated Secured Debt
//	             Air Freight and Logistics Acme Bidco Inc. Asset Type First
//	             Lien Term Loan Reference Rate and Spread S + 4.75% ...
//
// The last family runs a category, an industry, the name and the terms
// together with nothing between them. The name is recovered by peeling known
// categories and industries off the front and cutting at the first field
// label or instrument phrase.
//
// Where the parser must err, it prefers a name that keeps a stray word over
// one cut in half: the former can still be matched to its other spellings.
package ident

import (
	"regexp"
	"strings"
)

// Lien is the seniority class of a claim.
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

// IsDebt reports whether the class carries a par amount a price can be read
// against. Structured vehicles and equity are excluded: their "principal" is
// not a claim on the borrower.
func (l Lien) IsDebt() bool { return l != Equity && l != Structured }

// Facility distinguishes funded term debt from commitments that draw and repay.
type Facility uint8

const (
	Term Facility = iota
	Revolver
	DelayedDraw
)

var facilityNames = [...]string{"term", "revolver", "delayed_draw"}

func (f Facility) String() string { return facilityNames[f] }

// Parsed is the structure recovered from one identifier.
type Parsed struct {
	Borrower string
	Aliases  []string
	// Descriptor is everything in the identifier that is not the name.
	Descriptor string
	Lien       Lien
	Facility   Facility
	// Industry is the industry label found in the identifier, if any.
	Industry string
	// Unfunded marks a row that reports a commitment not yet drawn. Its
	// "principal" is the undrawn amount and its fair value is not a price.
	Unfunded bool
}

// Nameless reports whether the parser found no usable borrower: nothing, or
// only the words headings and instruments are made of. Such rows cannot be
// matched to anything and would otherwise all collect under one false name.
func (p Parsed) Nameless() bool {
	if len(words(p.Borrower)) == 0 || isBoilerplate(p.Borrower) || isIndustry(p.Borrower) {
		return true
	}
	// A total, not a holding: "Net Assets 10.4%".
	if lower := strings.ToLower(p.Borrower); strings.Contains(lower, "net assets") || strings.Contains(lower, "%") {
		return true
	}
	// "Term Loan", "Senior Secured Notes": an instrument with no company in it.
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
	// Column headings and legal forms that turn up alone where a name should be.
	"maturity": true, "acquisition": true, "services": true, "debt": true, "investment": true,
	"investments": true, "inc": true, "corp": true, "ltd": true, "co": true, "company": true,
	"holdings": true, "and": true,
}

var (
	// "PatientPoint Health Technologies, LLC Acquisition Date 03/30/21".
	acquiredRe = regexp.MustCompile(`(?i)\s+(?:acquisition date|investment\s?type|maturity date|interest rate).*$`)
	spaceRe    = regexp.MustCompile(`\s+`)
	// Footnote references a filer left attached to a name: (1), (12), (a), [3].
	footRe = regexp.MustCompile(`\s*(\(\s*\d{1,2}\s*\)|\(\s*[a-z]\s*\)|\[\s*\d{1,2}\s*\])`)
	// "(dba Acme Software)", "(f/k/a Old Name, Inc.)" and friends.
	aliasRe = regexp.MustCompile(`(?i)\(\s*(?:d/?b/?a|f/?k/?a|a/?k/?a|formerly known as|formerly|also known as)\.?:?\s+([^)]+)\)`)
	// Any other parenthesis after a name: "(Fairbanks Morse)", "(Delayed Draw)".
	parenRe = regexp.MustCompile(`\(([^()]*)\)`)
	// A rate or date fragment: "5.75%", "06/2029", "S+575".
	termRe = regexp.MustCompile(`(?i)\d\s*%|\d{1,2}/\d{2,4}|\b[sl]\s*\+\s*\d|\bsofr\b|\blibor\b|\bprime\b|\beuribor\b|\bsonia\b`)
	// A trailing position number, or a year: "Acme Corp 2", "Acme Corp 2025".
	indexRe = regexp.MustCompile(`\s+(\d{1,2}|20\d\d)$`)
	// A trailing position number spelled out, as some filers do: "... One".
	spelledRe = regexp.MustCompile(`(?i)\s+(one|two|three|four|five|six|seven|eight|nine|ten)$`)
	// A percentage-of-net-assets heading: "- 195.3% ".
	headingRe = regexp.MustCompile(`[-\x{2013}\x{2014}]\s*\d+(?:\.\d+)?\s*%\s*[-\x{2013}\x{2014}]?\s+`)
	// Marks of an undrawn commitment.
	unfundedRe = regexp.MustCompile(`(?i)\bcommitment (type|expiration)\b|\bunfunded\b|\bundrawn\b`)
	// A field label run into the word before it: "LLCType of Investment".
	gluedRe = regexp.MustCompile(`([A-Za-z.,)])(Type of Investment|Investment Type|Asset Type|Maturity Date|Interest Rate|Reference Rate)`)
	// Two headings run together: "Investmentsnon-controlled".
	runOnHeadingRe = regexp.MustCompile(`(?i)(investments?)(non[-\s]*control)`)
	// A heading that can sit in the middle of an identifier, after the name
	// of the fund or subsidiary the schedule belongs to. The borrower follows.
	midHeadingRe = regexp.MustCompile(`(?i)\b(?:non[-\s]*)?control(?:led)?\s*/\s*(?:non[-\s]*)?affiliat\w*\s+(?:investments?|issuers?|portfolio\s+compan(?:y|ies))\b[:\s]*`)
	// A name given with its trading name and no parentheses.
	inlineAliasRe = regexp.MustCompile(`(?i)\s+(?:d/b/a|dba|f/k/a|fka)\s+(.+)$`)
	// Words that qualify a facility and trail the name once the facility
	// itself has been cut away: "Acme LLC 2022 Incremental [Term Loan]".
	trailingRe = regexp.MustCompile(`(?i)\s+(?:incremental|initial|replacement|extended|refinancing|add-on|amended|closing date|super priority|priority|tranche\s+[a-z0-9-]{1,3})$`)
	// A parenthesis the source cut off before it closed.
	openParenRe = regexp.MustCompile(`\s*\([^)]*$`)
	pipeRe      = regexp.MustCompile(`\s*\|\s*`)
	dashRe      = regexp.MustCompile(`\s+[\x{2013}\x{2014}-]\s+`)
)

// legalSuffix lists segments that belong to a name even though they stand
// alone after a comma, as in "Associations, Inc.".
var legalSuffix = map[string]bool{
	"inc": true, "inc.": true, "incorporated": true, "llc": true, "l.l.c.": true,
	"lp": true, "l.p.": true, "llp": true, "ltd": true, "ltd.": true, "limited": true,
	"corp": true, "corp.": true, "corporation": true, "co": true, "co.": true,
	"plc": true, "gmbh": true, "s.a.": true, "sa": true, "b.v.": true, "bv": true,
	"n.v.": true, "nv": true, "ag": true, "pty": true, "ulc": true, "s.a.r.l.": true,
	"sarl": true, "s.à r.l.": true, "s.a r.l.": true, "pllc": true, "p.c.": true, "lllp": true,
}

// companyWords are words that mark a segment as a company's name when there
// is a choice of segments. A legal form, or a rung of an acquisition vehicle.
var companyWords = map[string]bool{
	"inc": true, "llc": true, "lp": true, "llp": true, "ltd": true, "limited": true, "corp": true,
	"corporation": true, "co": true, "company": true, "plc": true, "gmbh": true, "sarl": true, "bv": true,
	"nv": true, "ag": true, "pty": true, "ulc": true, "holdings": true, "holdco": true, "bidco": true,
	"topco": true, "midco": true, "parent": true, "buyer": true, "purchaser": true, "borrower": true,
	"intermediate": true, "merger": true, "group": true, "partners": true,
}

// strong phrases identify an instrument unambiguously wherever they appear,
// so they are safe to cut a run-together identifier on.
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

// labels are the field names some filers write into the identifier itself.
// The name always ends before the first of them.
var labels = []string{
	"asset type", "investment type", "type of investment", "instrument", "commitment type",
	"industry", "reference rate", "interest rate", "interest term", "maturity",
	"initial acquisition", "investment date", "current coupon", "basis point", "total coupon",
	"effective yield", "sofr", "libor", "euribor", "sonia",
}

// investmentNext lists the words that, following "Investment", show it to be
// a field label ("Acme, Inc. Investment First Lien Debt") and not part of a
// name ("Acme Investment Holdings").
var investmentNext = map[string]bool{
	"first": true, "second": true, "senior": true, "junior": true, "unitranche": true,
	"subordinated": true, "preferred": true, "common": true, "equity": true, "warrants": true,
	"secured": true, "unsecured": true, "1st": true, "2nd": true, "mezzanine": true,
}

// weak words mark a descriptor only in a segment after the first: they are
// ordinary enough to appear inside a company name.
var weak = []string{
	"loan", "loans", "note", "notes", "bond", "bonds", "debenture", "debt",
	"preferred", "common", "equity", "units", "unit", "warrant", "shares",
	"stock", "interest", "interests", "tranche", "facility", "incremental",
	"series", "class", "prime", "floor", "pik", "due", "maturity", "matures",
	"ddtl", "unsecured", "secured", "senior", "junior", "lien", "affiliated", "issuer",
}

// boilerplate words make up the headings filers group their schedule under:
// "Non-Controlled, Non-Affiliated Debt Investments", "US Corporate Debt",
// "Funded Investments - First lien". A segment made of nothing else is a
// heading, never a name.
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

// prefixRes strip one heading from the front of a run-together identifier.
// They are applied repeatedly until none matches.
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

// countryRe strips a country, but only once a heading has already been
// stripped: "United States" after "Debt Investments" is a heading, at the
// start of an identifier it could be a company.
var countryRe = regexp.MustCompile(`(?i)^(?:united\s+states|united\s+kingdom|australia|canada|cayman\s+islands|luxembourg|germany|france|netherlands|ireland|sweden|europe|israel|singapore|bermuda|jersey|switzerland|belgium|spain|italy|denmark|norway|finland|new\s+zealand)\b`)

// headingStart recognises an identifier that opens with a heading and so
// needs the run-together treatment.
//
// The filings drop a leading letter often enough ("ortfolio Investments",
// "on-Controlled") that each alternative allows for it.
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

// isBoilerplate reports whether a segment is a heading: made only of the
// words headings are made of.
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

// nameEnd finds where a name stops inside a run of text: at the first field
// label, instrument phrase or spaced dash, provided something precedes it.
// It returns len(s) when nothing marks an end.
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
			// Whole words only, so "Sofranko" is not read as a SOFR margin
			// and "Parexel" not as a par amount.
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
	// "Acquisition" is a label only when a date follows it; "Investment" only
	// when an instrument does. Otherwise both are words in a company's name.
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

// stripHeadings peels headings, a country and an industry off the front of a
// run-together identifier, leaving it starting at the name.
func stripHeadings(s, holder string) (rest, industry string) {
	// Percentage headings nest ("Debt Investments - 179% United States - 161%
	// 1st Lien - 155% Acme"); the name follows the last of them.
	if m := headingRe.FindAllStringIndex(s, -1); m != nil {
		s = s[m[len(m)-1][1]:]
	}
	// "Acme Credit Subsidiary, LLC BANK LOANS: NON-CONTROL/NON-AFFILIATE
	// INVESTMENTS Capital Equipment Borrower Co": the borrower follows the
	// last such heading, whatever entity is named before it.
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
		// Some filers print their own name as a heading above the schedule.
		// EDGAR records it in capitals and without the commas, so compare
		// word by word.
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
		// A heading can follow the industry too: "... Software Common Stock Acme".
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

// dedupe drops a short name repeated in front of the legal name, as in
// "Accel International Accel International Holdings, LLC".
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

// tidy finishes a name: drops footnotes, a trailing position number and
// stray punctuation, and moves a parenthesised short name to the aliases.
const debtHeading = "debt investments"

// TrimLabels cuts a column label, and whatever follows it, off the end of a
// name: "Acme, LLC Acquisition Date 03/30/21" is "Acme, LLC".
func TrimLabels(name string) string {
	return strings.TrimSpace(acquiredRe.ReplaceAllString(name, ""))
}

func tidy(name string, p *Parsed) string {
	// A stray mark and a section heading before the name, and a date after it.
	name = strings.TrimLeft(name, "?*• 	")
	// "... Debt Investments Technology Hardware & Equipment Brivo, Inc.": one
	// filer runs the schedule's headings, the country and the industry into
	// the name. The name is what follows the last of them.
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
		// "(Delayed Draw)" describes the facility; "(Fairbanks Morse)" is
		// what the borrower is known as.
		if inner != "" && !descriptorLike(inner) && !isDigits(inner) {
			p.Aliases = append(p.Aliases, inner)
		}
	}
	name = parenRe.ReplaceAllString(name, "")
	name = openParenRe.ReplaceAllString(name, "")
	name = spaceRe.ReplaceAllString(strings.TrimSpace(name), " ")
	// The cut can land inside a parenthesis: "Acme LLC (Delayed Draw)".
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

// Parse extracts the borrower and instrument class from an identifier. The
// typeAxis argument is the Investment Type axis member for the same row.
func Parse(identifier, typeAxis string) Parsed {
	return ParseFor(identifier, typeAxis, "")
}

// ParseFor is Parse for a known registrant, whose own name some filers print
// as a heading inside each identifier.
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
		// "First Lien Secured Debt Issuer Name Acme, LLC Acquisition ..."
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
	// A securitisation or a joint venture is recognisable from its name alone.
	lower := strings.ToLower(p.Borrower)
	if hasWord(lower, "clo") || hasWord(lower, "jv") || strings.Contains(lower, "senior loan program") ||
		strings.Contains(lower, "senior loan fund") || strings.Contains(lower, "joint venture") {
		p.Lien = Structured
	}
	return p
}

// hasLabel reports whether an identifier carries field labels, which only
// the run-together family does.
func hasLabel(s string) bool {
	l := strings.ToLower(s)
	for _, x := range []string{" asset type ", " investment type ", " type of investment ", " instrument ", " reference rate", " maturity date ", " interest rate "} {
		if strings.Contains(l, x) {
			return true
		}
	}
	return false
}

// fromSegments picks the name out of delimited segments. Headings, industry
// labels and terms are passed over; of what remains, the first segment is
// the name unless a later one is plainly a company and it is not.
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
			continue // the name is decided; only an industry is still wanted
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

// fromCommas reads the common grammar, "Name, instrument, terms". The name is
// the first segment plus any bare legal suffix that follows it.
//
// A few filers lead with the facility and the country instead ("Term Loan,
// United States of America, Acme LLC, Healthcare, Rate S+7%"). Leading
// segments that are plainly not a company are passed over, provided a later
// one plainly is.
func fromCommas(s string) (name, industry string) {
	segs := strings.Split(s, ",")
	for i := range segs {
		segs[i] = strings.TrimSpace(segs[i])
	}
	// "LLC 1": a legal suffix can carry the filer's position number.
	suffix := func(seg string) bool {
		return legalSuffix[strings.ToLower(indexRe.ReplaceAllString(seg, ""))]
	}
	company := func(i int) bool {
		return hasCompanyWord(segs[i]) || (i+1 < len(segs) && suffix(segs[i+1]))
	}
	// Only a segment with no company in it at all is passed over: a heading,
	// a country, an industry, or an instrument with nothing else ("Term
	// Loan"). "Opal Facility Services" has an instrument word in it and is
	// still a name.
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
	// "Smith, Jones & Partners, LLC": a legal suffix further on means the
	// segments before it are still the name.
	for j := end; j < len(segs) && j <= end+1; j++ {
		if suffix(segs[j]) && !descriptorLike(segs[end]) {
			end = j + 1
			break
		}
	}
	name = strings.Join(segs[first:end], ", ")
	// Filers that never use commas leave the whole identifier in one piece.
	if end == first+1 {
		name = name[:nameEnd(name)]
	}
	return name, industry
}

// Classify reads seniority and facility type from the type axis and the
// descriptor. The type axis wins on seniority; the descriptor is consulted
// for whatever the axis leaves open.
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
	// Order matters. "unsecured notes" contains "secured note", so the junior
	// classes are tested before the senior ones they would otherwise match.
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

// camelSplit turns an XBRL member name such as "DebtSecuritiesFirstLienMember"
// into spaced words so phrase matching works on it.
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
