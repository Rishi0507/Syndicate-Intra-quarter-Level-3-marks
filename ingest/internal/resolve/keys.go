// Package resolve decides which borrower names, written differently by
// different lenders, refer to the same company.
//
// Everything downstream compares marks across lenders, so a false merge is the
// expensive mistake: it manufactures disagreement between two loans that were
// never the same loan. The rules here are therefore biased toward leaving
// names apart unless either the names agree after normalisation or the loan
// terms the lenders report corroborate the match.
package resolve

import (
	"strings"
	"unicode"
)

// legal words carry no identity: "Acme, Inc." and "Acme LLC" are one company.
var legal = set("inc", "incorporated", "llc", "lp", "llp", "lllp", "ltd", "limited",
	"corp", "corporation", "co", "company", "plc", "gmbh", "sarl", "sa", "bv", "nv",
	"ag", "pty", "ulc", "lc", "pllc", "pc", "se", "spa", "srl", "ab", "oy", "kg")

// structure words name a rung of an acquisition vehicle, not the business.
// A sponsor-backed borrower shows up as "Acme Buyer", "Acme Intermediate
// Holdings" and "Acme Parent" depending on which entity a lender faces.
var structure = set("holding", "holdco", "parent", "intermediate", "buyer", "bidco",
	"topco", "midco", "acquisition", "acquiror", "acquirer", "acquireco", "merger",
	"sub", "purchaser", "borrower", "issuer", "opco", "newco", "aggregator",
	"investco", "blocker", "ultimate", "finco", "i", "ii", "iii", "iv", "v", "vi")

var stop = set("the", "and", "of", "for")

// abbreviations lenders use interchangeably with the full word.
var expand = map[string]string{
	"intl": "international", "int": "international", "svc": "service",
	"svcs": "service", "mgmt": "management", "hldg": "holding", "hldgs": "holding",
	"grp": "group", "tech": "technology", "mfg": "manufacturing",
	"sys": "system", "assoc": "associate", "natl": "national", "amer": "american",
	"hlth": "health", "healthcare": "health", "pharm": "pharmaceutical",
	"labs": "laboratory", "lab": "laboratory", "ctr": "center", "centre": "center",
	"dept": "department", "dev": "development", "dist": "distribution",
	"ent": "enterprise", "fin": "financial", "ins": "insurance", "inds": "industry",
	"ind": "industry", "prod": "product", "prods": "product", "sol": "solution",
	"solns": "solution", "comm": "communication", "comms": "communication",
	"telecom": "telecommunication", "us": "us", "usa": "us",
}

func set(words ...string) map[string]bool {
	m := make(map[string]bool, len(words))
	for _, w := range words {
		m[w] = true
	}
	return m
}

var fold = strings.NewReplacer(
	// Longest first: the replacer takes the first pattern that matches at a position.
	"s.à r.l.", " sarl ", "s.a r.l.", " sarl ", "s.a.r.l.", " sarl ",
	"à", "a", "á", "a", "â", "a", "ä", "a", "ã", "a", "å", "a",
	"è", "e", "é", "e", "ê", "e", "ë", "e",
	"ì", "i", "í", "i", "î", "i", "ï", "i",
	"ò", "o", "ó", "o", "ô", "o", "ö", "o", "õ", "o", "ø", "o",
	"ù", "u", "ú", "u", "û", "u", "ü", "u",
	"ç", "c", "ñ", "n", "ß", "ss",
	"&", " and ", "+", " and ", "'", "", "’", "", ".", "",
)

// stem folds plurals so "Technologies" and "Technology" share a token.
func stem(t string) string {
	n := len(t)
	switch {
	case n > 4 && strings.HasSuffix(t, "ies"):
		return t[:n-3] + "y"
	case n > 3 && strings.HasSuffix(t, "s") &&
		!strings.HasSuffix(t, "ss") && !strings.HasSuffix(t, "us") && !strings.HasSuffix(t, "is"):
		return t[:n-1]
	}
	return t
}

// Tokens lowercases a name, strips punctuation and returns its normalised
// words with legal suffixes and stop words removed.
func Tokens(name string) []string {
	s := fold.Replace(strings.ToLower(name))
	fields := strings.FieldsFunc(s, func(r rune) bool {
		return !unicode.IsLetter(r) && !unicode.IsDigit(r)
	})
	out := fields[:0]
	for _, f := range fields {
		if e, ok := expand[f]; ok {
			f = e
		}
		f = stem(f)
		if legal[f] || stop[f] {
			continue
		}
		out = append(out, f)
	}
	// Single letters left over from "S.à r.l." style suffixes are noise at the
	// end of a name but meaningful at the start ("A Place for Mom").
	for len(out) > 1 && len(out[len(out)-1]) == 1 {
		out = out[:len(out)-1]
	}
	return out
}

// Strict is the name with only legal form removed.
func Strict(name string) string { return strings.Join(Tokens(name), " ") }

// CoreTokens additionally removes acquisition-vehicle words and a trailing
// position number. If that would leave nothing, the strict tokens are kept:
// "Intermediate Holdings LLC" has no core to fall back on.
func CoreTokens(name string) []string {
	toks := Tokens(name)
	out := make([]string, 0, len(toks))
	for _, t := range toks {
		if !structure[t] {
			out = append(out, t)
		}
	}
	for len(out) > 1 && isSmallInt(out[len(out)-1]) {
		out = out[:len(out)-1]
	}
	if len(out) == 0 {
		return toks
	}
	return out
}

// Core is the business name with legal form and deal structure removed.
func Core(name string) string { return strings.Join(CoreTokens(name), " ") }

func isSmallInt(t string) bool {
	if len(t) == 0 || len(t) > 2 {
		return false
	}
	for i := 0; i < len(t); i++ {
		if t[i] < '0' || t[i] > '9' {
			return false
		}
	}
	return true
}
