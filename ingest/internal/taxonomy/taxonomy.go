// Package taxonomy maps the labels filers invent onto two small shared
// vocabularies: an industry sector for each borrower and a manager family for
// each lender.
package taxonomy

import (
	"strings"
	"unicode"
)

// Sector is a canonical industry bucket.
type Sector struct {
	Code  string
	Label string
}

// Sectors is the full vocabulary, in display order.
var Sectors = []Sector{
	{"software", "Software & Technology"},
	{"healthcare", "Healthcare"},
	{"business", "Business Services"},
	{"financial", "Financials & Insurance"},
	{"consumer", "Consumer"},
	{"industrial", "Industrials"},
	{"aerospace", "Aerospace & Defense"},
	{"energy", "Energy & Utilities"},
	{"media", "Media & Telecom"},
	{"transport", "Transportation & Logistics"},
	{"realestate", "Real Estate & Construction"},
	{"education", "Education"},
	{"food", "Food & Beverage"},
	{"materials", "Chemicals & Materials"},
	{"other", "Other"},
}

// sectorRules are tried in order and the first hit wins, so the order encodes
// precedence: "Health Care Technology" is healthcare, "Electronic Equipment"
// is technology rather than a utility, "Consumer Finance" is a financial.
var sectorRules = []struct {
	code string
	any  []string
}{
	{"aerospace", []string{"aerospace", "defense"}},
	{"healthcare", []string{"health", "pharma", "medical", "biotech", "life science", "dental", "hospital", "drug"}},
	{"software", []string{"software", "technology", "high tech", "internet", " it service", "information",
		"data processing", "semiconductor", "electronic", "cyber", "digital", "computer"}},
	{"financial", []string{"financ", "insurance", "bank", "asset management", "capital market", "mortgage"}},
	{"education", []string{"education"}},
	{"food", []string{"food", "beverage", "grocery", "agricultur", "tobacco"}},
	{"media", []string{"media", "telecom", "broadcast", "publishing", "advertising", "communication",
		"entertainment", "cable", "wireless"}},
	{"transport", []string{"transport", "logistic", "cargo", "freight", "shipping", "airline", "marine", " rail"}},
	{"energy", []string{"energy", " oil", " gas ", "utilit", "power", "renewable", "midstream"}},
	{"realestate", []string{"real estate", "construction", "building", "homebuilding", "engineering"}},
	{"materials", []string{"chemical", "material", "metal", "mining", "packaging", "container", "paper",
		"plastic", "rubber", "forest", "glass"}},
	{"consumer", []string{"consumer", "retail", "leisure", "hotel", "gaming", "apparel", "household",
		"personal", "lodging", "durable", "recreation", "fitness", "textile", "luxury", "restaurant"}},
	{"industrial", []string{"industrial", "manufactur", "machinery", "capital equipment", "capital goods",
		"auto", "equipment", "environmental", "wholesale", "distribut", "trading compan"}},
	{"business", []string{"business", "commercial service", "professional", "service", "human resource",
		"consulting", "staffing"}},
}

// spaced lowercases a label and replaces punctuation with spaces. With camel
// set it also breaks XBRL member names into words, so "HealthCareSectorMember"
// and "Health Care" look alike. Registrant names are left whole: splitting
// "PennantPark" or "BlackRock" would hide them from the family list.
func spaced(s string, camel bool) string {
	var b strings.Builder
	var prev rune
	for _, r := range s {
		if camel && unicode.IsUpper(r) && unicode.IsLower(prev) {
			b.WriteByte(' ')
		}
		if unicode.IsLetter(r) || unicode.IsDigit(r) {
			b.WriteRune(unicode.ToLower(r))
		} else {
			b.WriteByte(' ')
		}
		prev = r
	}
	return " " + strings.Join(strings.Fields(b.String()), " ") + " "
}

// SectorOf returns the canonical sector code for a filer's industry label,
// or "" when the label is empty.
func SectorOf(label string) string {
	if strings.TrimSpace(label) == "" {
		return ""
	}
	s := spaced(label, true)
	for _, r := range sectorRules {
		for _, k := range r.any {
			if strings.Contains(s, k) {
				return r.code
			}
		}
	}
	return "other"
}

// nameHints are words in a company's own name that give its sector away.
// Only whole words count, and only words that rarely mislead: "Dental" and
// "Insurance" are in, "Technologies" is not.
var nameHints = map[string]string{
	"software": "software", "cloud": "software", "cyber": "software", "data": "software", "analytics": "software",
	"health": "healthcare", "healthcare": "healthcare", "medical": "healthcare", "dental": "healthcare",
	"pharma": "healthcare", "pharmaceutical": "healthcare", "pharmaceuticals": "healthcare", "biosciences": "healthcare",
	"therapeutics": "healthcare", "veterinary": "healthcare", "vet": "healthcare", "hospice": "healthcare",
	"dermatology": "healthcare", "orthopedic": "healthcare", "clinical": "healthcare", "dentist": "healthcare",
	"insurance": "financial", "financial": "financial", "bank": "financial", "wealth": "financial", "advisors": "financial",
	"logistics": "transport", "freight": "transport", "transportation": "transport", "aviation": "transport",
	"aerospace": "aerospace", "defense": "aerospace",
	"energy": "energy", "solar": "energy", "power": "energy", "utilities": "energy",
	"foods": "food", "food": "food", "beverage": "food", "brewing": "food", "restaurant": "consumer", "restaurants": "consumer",
	"education": "education", "learning": "education", "school": "education", "schools": "education",
	"media": "media", "telecom": "media", "wireless": "media", "fiber": "media", "broadband": "media",
	"packaging": "materials", "chemical": "materials", "chemicals": "materials", "plastics": "materials",
	"construction": "realestate", "roofing": "realestate", "hvac": "realestate",
	"automotive": "industrial", "manufacturing": "industrial", "industrial": "industrial", "industries": "industrial",
	"fitness": "consumer", "retail": "consumer", "brands": "consumer", "pet": "consumer",
	"staffing": "business", "consulting": "business",
}

// SectorOfName guesses a sector from a borrower's name. It returns "" when
// the name gives nothing away, which is most of the time.
func SectorOfName(name string) string {
	for _, w := range strings.Fields(spaced(name, false)) {
		if code, ok := nameHints[w]; ok {
			return code
		}
	}
	return ""
}

// families groups registrants that share a valuation process. Vehicles run by
// one manager mark a shared loan with one committee, so agreement between them
// is not independent evidence and must not be counted as such.
var families = []struct {
	label string
	any   []string
}{
	{"Apollo", []string{"apollo", "midcap financial investment", " mfic "}},
	{"Ares", []string{" ares "}},
	{"Blue Owl", []string{"blue owl", "owl rock"}},
	{"Blackstone", []string{"blackstone"}},
	{"Golub", []string{"golub", " gbdc "}},
	{"FS / KKR", []string{"fs kkr", " kkr ", "fs investment", "fs energy", "fs specialty", "k fit", "kkr fs"}},
	{"Goldman Sachs", []string{"goldman sachs", "west bay bdc", "phillip street middle market"}},
	{"Morgan Stanley", []string{"morgan stanley", "north haven", "sl investment", "t series middle market"}},
	{"HPS", []string{" hps "}},
	{"Oaktree", []string{"oaktree"}},
	{"Carlyle", []string{"carlyle", "tcg bdc"}},
	{"Bain Capital", []string{"bain capital"}},
	{"New Mountain", []string{"new mountain", "nmf slf"}},
	{"Sixth Street", []string{"sixth street", "tpg specialty"}},
	{"BlackRock", []string{"blackrock", "tcp capital"}},
	{"Barings", []string{"barings"}},
	{"Franklin BSP", []string{"franklin bsp", "benefit street", "business development corp of america",
		"business development corporation of america"}},
	{"Nuveen Churchill", []string{"nuveen churchill", "nc slf", "churchill middle market"}},
	{"Antares", []string{"antares"}},
	{"T. Rowe Price OHA", []string{"t rowe price oha", "oha senior", " oha "}},
	{"Monroe", []string{"monroe capital"}},
	{"Kayne Anderson", []string{"kayne"}},
	{"Stone Point", []string{"stone point"}},
	{"PennantPark", []string{"pennantpark"}},
	{"Prospect", []string{"prospect capital", "prospect flexible", "prospect floating"}},
	{"Main Street", []string{"main street", "msc income"}},
	{"Fidelity", []string{"fidelity"}},
	{"Stellus", []string{"stellus"}},
	{"Crescent", []string{"crescent capital", "crescent private"}},
	{"Brightwood", []string{"brightwood"}},
	{"Onex", []string{"onex"}},
	{"TPG Twin Brook", []string{"twin brook", " agtb "}},
	{"SLR", []string{" slr "}},
	{"Hercules", []string{"hercules"}},
	{"Trinity", []string{"trinity capital"}},
	{"Capital Southwest", []string{"capital southwest"}},
	{"Gladstone", []string{"gladstone"}},
	{"Saratoga", []string{"saratoga"}},
	{"WhiteHorse", []string{"whitehorse"}},
	{"Fidus", []string{"fidus"}},
	{"Horizon", []string{"horizon technology"}},
	{"Runway", []string{"runway growth"}},
	{"TriplePoint", []string{"triplepoint"}},
	{"OFS", []string{" ofs "}},
	{"BC Partners", []string{"portman ridge", "bc partners", "logan ridge", "mount logan"}},
	{"Great Elm", []string{"great elm"}},
	{"Oxford Square", []string{"oxford square"}},
	{"CION", []string{" cion "}},
	{"BDT & MSD", []string{"msd investment", "bdt msd"}},
	{"Silver Point", []string{"silver point"}},
	{"Vista", []string{"vista credit"}},
	{"Jefferies", []string{"jefferies credit", "senior credit investments"}},
	{"Kennedy Lewis", []string{"kennedy lewis"}},
	{"Lafayette Square", []string{"lafayette square"}},
	{"Overland", []string{"overland advantage"}},
	{"Varagon", []string{"varagon"}},
	{"Palmer Square", []string{"palmer square"}},
	{"Audax", []string{"audax"}},
	{"Comvest", []string{"comvest"}},
	{"Diameter", []string{"diameter credit"}},
	{"26North", []string{"26north", "26 north"}},
	{"Lord Abbett", []string{" lgam ", "lord abbett"}},
	{"Manulife", []string{"john hancock", "manulife"}},
	{"StepStone", []string{"stepstone"}},
	{"Hamilton Lane", []string{"hamilton lane"}},
	{"PGIM", []string{"pgim"}},
	{"Nomura", []string{"nomura"}},
	{"TCW", []string{" tcw "}},
	{"WTI", []string{"venture lending leasing", "wti fund"}},
	{"Steele Creek", []string{"steele creek"}},
	{"Flat Rock", []string{"flat rock"}},
	{"Muzinich", []string{"muzinich"}},
	{"Eagle Point", []string{"eagle point"}},
	{"First Eagle", []string{"first eagle"}},
	{"Investcorp", []string{"investcorp"}},
	{"PhenixFIN", []string{"phenixfin"}},
}

// generic words say what kind of vehicle a registrant is, not whose it is.
var generic = map[string]bool{
	"capital": true, "corp": true, "corporation": true, "fund": true, "inc": true,
	"bdc": true, "lending": true, "credit": true, "income": true, "private": true,
	"llc": true, "lp": true, "trust": true, "investment": true, "investments": true,
	"strategic": true, "senior": true, "secured": true, "direct": true,
	"holdings": true, "finance": true, "partners": true, "technology": true,
	"growth": true, "opportunities": true, "ii": true, "iii": true, "iv": true,
	"v": true, "the": true, "company": true, "co": true, "middle": true,
	"market": true, "specialty": true, "solutions": true, "of": true, "and": true,
	"ltd": true, "group": true, "management": true, "debt": true, "loan": true,
}

// FamilyOf returns the manager family a registrant belongs to. Registrants
// outside the curated list fall back to the leading distinctive words of
// their own name, which keeps sibling vehicles together in the common case.
func FamilyOf(holder string) string {
	s := spaced(holder, false)
	for _, f := range families {
		for _, k := range f.any {
			if strings.Contains(s, k) {
				return f.label
			}
		}
	}
	var keep []string
	for _, w := range strings.Fields(s) {
		if generic[w] {
			continue
		}
		keep = append(keep, strings.ToUpper(w[:1])+w[1:])
		if len(keep) == 2 {
			break
		}
	}
	if len(keep) == 0 {
		return strings.TrimSpace(holder)
	}
	return strings.Join(keep, " ")
}
