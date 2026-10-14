package resolve

import (
	"strings"
	"unicode"
)

var legal = set("inc", "incorporated", "llc", "lp", "llp", "lllp", "ltd", "limited",
	"corp", "corporation", "co", "company", "plc", "gmbh", "sarl", "sa", "bv", "nv",
	"ag", "pty", "ulc", "lc", "pllc", "pc", "se", "spa", "srl", "ab", "oy", "kg")

var structure = set("holding", "holdco", "parent", "intermediate", "buyer", "bidco",
	"topco", "midco", "acquisition", "acquiror", "acquirer", "acquireco", "merger",
	"sub", "purchaser", "borrower", "issuer", "opco", "newco", "aggregator",
	"investco", "blocker", "ultimate", "finco", "i", "ii", "iii", "iv", "v", "vi")

var stop = set("the", "and", "of", "for")

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
	"s.à r.l.", " sarl ", "s.a r.l.", " sarl ", "s.a.r.l.", " sarl ",
	"à", "a", "á", "a", "â", "a", "ä", "a", "ã", "a", "å", "a",
	"è", "e", "é", "e", "ê", "e", "ë", "e",
	"ì", "i", "í", "i", "î", "i", "ï", "i",
	"ò", "o", "ó", "o", "ô", "o", "ö", "o", "õ", "o", "ø", "o",
	"ù", "u", "ú", "u", "û", "u", "ü", "u",
	"ç", "c", "ñ", "n", "ß", "ss",
	"&", " and ", "+", " and ", "'", "", "’", "", ".", "",
)

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
	for len(out) > 1 && len(out[len(out)-1]) == 1 {
		out = out[:len(out)-1]
	}
	return out
}

func Strict(name string) string { return strings.Join(Tokens(name), " ") }

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
