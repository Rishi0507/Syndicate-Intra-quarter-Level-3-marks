// Package synth generates a private credit universe with a known answer and
// writes it in the SEC's own file format.
//
// It exists for two reasons. The pipeline and the kernel can be tested end to
// end without network access, and the kernel can be scored against the true
// latent value of every loan, which no real data set will ever disclose.
//
// The universe is deliberately awkward in the ways real filings are: lenders
// name the same borrower differently, write identifiers in different grammars,
// file on different days, repeat prior periods as comparatives, and mix in
// equity, unfunded commitments and non-dollar loans that must be filtered out.
// Nothing here is, or is derived from, a real company or a real mark.
package synth

import (
	"archive/zip"
	"bufio"
	"fmt"
	"math"
	"math/rand"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"syndicate/ingest/internal/day"
	"syndicate/ingest/internal/resolve"
	"syndicate/ingest/internal/taxonomy"
)

// Config sizes the universe. The zero value is not useful; use Default.
type Config struct {
	Seed      int64
	Families  int
	Borrowers int
	Start     day.Date // first day of the latent process
	End       day.Date // last day of the latent process
}

// Default is a universe large enough to exercise every code path and small
// enough to generate in a few seconds.
func Default() Config {
	return Config{Seed: 20260513, Families: 34, Borrowers: 1500,
		Start: day.New(2022, 7, 1), End: day.New(2026, 9, 30)}
}

// Truth is the answer key written alongside the SEC-format archives.
type Truth struct {
	// Entity maps "cik|identifier" to the true borrower index.
	Entity map[string]int
	// Core is each borrower's normalised name, the join key to the panel.
	Core []string
}

var prefixes = strings.Fields(`Apex Summit Northwind Cobalt Meridian Harbor Granite Sterling Vantage
Keystone Beacon Cascade Ironwood Lumen Orchard Pinnacle Quarry Redwood Sable Tidewater Umbra Verity
Westbrook Yarrow Zenith Aldergate Brightline Cinder Driftwood Ember Fathom Gable Halcyon Indigo
Juniper Kestrel Larkspur Mosaic Nimbus Onyx Palisade Quill Rookery Sandbar Talon Upland Vesper
Willow Xenon Yonder Zephyr Anchor Birch Copper Dune Elm Flint Grove Heron Isle Jade Knoll Ledger
Maple Nettle Oak Pier Quince Ridge Slate Thorn Umber Vale Wren Arbor Basalt Cairn Delta Echo
Fjord Glade Haven Inlet Jetty Kiln Loom Marsh Notch Opal Plume Reef Spire Tarn Vault Wharf`)

var businesses = []struct {
	name   string
	sector string
}{
	{"Software", "software"}, {"Data Systems", "software"}, {"Cloud Solutions", "software"},
	{"Analytics", "software"}, {"Payments Technology", "software"}, {"Cyber Defense", "software"},
	{"Health Partners", "healthcare"}, {"Dental Management", "healthcare"}, {"Veterinary Group", "healthcare"},
	{"Physician Services", "healthcare"}, {"Pharma Services", "healthcare"}, {"Home Health", "healthcare"},
	{"Staffing", "business"}, {"Consulting Group", "business"}, {"Facility Services", "business"},
	{"Compliance Services", "business"}, {"Marketing Services", "business"},
	{"Insurance Services", "financial"}, {"Wealth Advisors", "financial"}, {"Specialty Finance", "financial"},
	{"Brands", "consumer"}, {"Fitness", "consumer"}, {"Pet Products", "consumer"}, {"Retail Group", "consumer"},
	{"Industrial Products", "industrial"}, {"Precision Manufacturing", "industrial"},
	{"Packaging", "materials"}, {"Specialty Chemicals", "materials"},
	{"Aerospace Components", "aerospace"}, {"Defense Systems", "aerospace"},
	{"Energy Services", "energy"}, {"Power Solutions", "energy"},
	{"Media", "media"}, {"Telecom Infrastructure", "media"},
	{"Logistics", "transport"}, {"Freight Systems", "transport"},
	{"Building Products", "realestate"}, {"Engineering Services", "realestate"},
	{"Learning", "education"}, {"Education Partners", "education"},
	{"Foods", "food"}, {"Beverage Company", "food"},
}

// sectorVocab holds two house vocabularies for the same sectors, so the
// pipeline has to map labels rather than match them.
var sectorVocab = map[string][2]string{
	"software":   {"Software", "High Tech Industries"},
	"healthcare": {"Health Care Providers & Services", "Healthcare & Pharmaceuticals"},
	"business":   {"Professional Services", "Services: Business"},
	"financial":  {"Insurance", "Banking, Finance, Insurance & Real Estate"},
	"consumer":   {"Specialty Retail", "Consumer Goods: Non-Durable"},
	"industrial": {"Machinery", "Capital Equipment"},
	"materials":  {"Containers & Packaging", "Chemicals, Plastics & Rubber"},
	"aerospace":  {"Aerospace & Defense", "Aerospace & Defense"},
	"energy":     {"Energy Equipment & Services", "Energy: Oil & Gas"},
	"media":      {"Diversified Telecommunication Services", "Media: Broadcasting & Subscription"},
	"transport":  {"Air Freight & Logistics", "Transportation: Cargo"},
	"realestate": {"Building Products", "Construction & Building"},
	"education":  {"Diversified Consumer Services - Education", "Education"},
	"food":       {"Food Products", "Beverage, Food & Tobacco"},
}

var vehicles = []string{"", "", "Buyer", "Holdings", "Intermediate Holdings", "Parent", "Acquisition", "Midco"}
var suffixes = []string{"Inc.", "LLC", "L.P.", "Corp.", "Holdings, LLC", "Inc"}

var familyNames = strings.Fields(`Aurelian Borealis Calder Dunmore Everhart Fennimore Galloway Harrowgate
Ivers Jessamine Kilbride Lachlan Marlowe Northcote Oakhurst Penhallow Quennell Rosthern Selkirk
Thackeray Underhill Varnham Wexcombe Yardley Ashdown Bexley Corrin Dalby Eastgate Farrow Glenmoor
Hadleigh Inverness Kendrick Lindqvist Merriweather`)

var vehicleKinds = []string{"Capital Corp", "Private Credit Fund", "Senior Lending Fund", "Strategic Income Fund", "Direct Lending Corp"}

type borrower struct {
	base, alias  string
	sector       string
	senior       []float64 // daily true price, 1.0 = par
	junior       []float64
	hasJunior    bool
	spreadBp     int
	jrSpreadBp   int
	maturity     day.Date
	startQ, endQ int
	size         float64
	foreign      bool
}

type family struct {
	name   string
	theta  float64 // share of the gap to fair value closed each quarter
	bias   float64 // level bias in price points
	noise  float64 // valuation noise at par
	style  int     // identifier grammar
	vocab  int     // sector vocabulary
	types  int     // how the type axis is populated
	lagQ   int     // days from quarter end to 10-Q
	lagK   int     // days from fiscal year end to 10-K
	fye    int     // fiscal year end month
	weight float64
	funds  []*fund
	// marks reported so far, per borrower and class, for the smoothing recursion
	last map[[2]int]float64
	// dist is the family's standing distance from fair value on each loan,
	// in units of its own noise.
	dist map[[2]int]float64
	// how this family spells each borrower
	spell map[int]string
}

type fund struct {
	cik, name string
	share     float64
	holds     map[int]float64 // borrower -> principal held
}

type gen struct {
	cfg     Config
	rng     *rand.Rand
	days    []day.Date
	dayIdx  map[day.Date]int
	qEnds   []day.Date
	baa     []float64
	bor     []*borrower
	fams    []*family
	truth   Truth
	reports map[[3]int]float64 // family, borrower, class*1000+quarter -> mark
}

func isWeekday(d day.Date) bool {
	wd := d.Time().Weekday()
	return wd >= 1 && wd <= 5
}

// Generate writes SEC-format archives into dir/sec, a matching factor table
// and the answer key into dir, and returns the answer key.
func Generate(cfg Config, dir string) (*Truth, error) {
	g := &gen{cfg: cfg, rng: rand.New(rand.NewSource(cfg.Seed)), dayIdx: map[day.Date]int{},
		reports: map[[3]int]float64{}}
	for d := cfg.Start; d <= cfg.End; d = d.AddDays(1) {
		if isWeekday(d) {
			g.dayIdx[d] = len(g.days)
			g.days = append(g.days, d)
		}
	}
	for y := cfg.Start.Year(); y <= cfg.End.Year(); y++ {
		for _, m := range []int{3, 6, 9, 12} {
			q := day.New(y, m, 1).MonthEnd()
			if q >= cfg.Start.AddDays(60) && q <= cfg.End.AddDays(-75) {
				g.qEnds = append(g.qEnds, q)
			}
		}
	}
	g.makeFactors()
	g.makeBorrowers()
	g.makeFamilies()
	g.assignHoldings()

	if err := os.MkdirAll(filepath.Join(dir, "sec"), 0o755); err != nil {
		return nil, err
	}
	if err := g.writeFactors(filepath.Join(dir, "factors.tsv")); err != nil {
		return nil, err
	}
	if err := g.writeArchives(filepath.Join(dir, "sec")); err != nil {
		return nil, err
	}
	if err := g.writeTruth(dir); err != nil {
		return nil, err
	}
	return &g.truth, nil
}

// lastIndexOnOrBefore maps a calendar date onto the weekday grid.
func (g *gen) at(d day.Date) int {
	for i := 0; i < 7; i++ {
		if k, ok := g.dayIdx[d.AddDays(-i)]; ok {
			return k
		}
	}
	return 0
}

// makeFactors builds a public credit spread with two stress episodes. It is a
// plausible driver, not a model of any actual period.
func (g *gen) makeFactors() {
	n := len(g.days)
	g.baa = make([]float64, n)
	x := 1.9
	for i := 0; i < n; i++ {
		t := float64(i) / float64(n)
		target := 1.75 + 0.9*math.Exp(-math.Pow((t-0.18)/0.05, 2)) + 1.1*math.Exp(-math.Pow((t-0.68)/0.035, 2))
		x += 0.045*(target-x) + 0.022*g.rng.NormFloat64()
		g.baa[i] = math.Max(0.9, x)
	}
}

func (g *gen) makeBorrowers() {
	n := len(g.days)
	nq := len(g.qEnds)

	sectorShock := map[string][]float64{}
	for _, s := range taxonomy.Sectors {
		sh := make([]float64, n)
		for i := range sh {
			sh[i] = 0.0003 * g.rng.NormFloat64()
		}
		sectorShock[s.Code] = sh
	}

	used := map[string]bool{}
	for len(g.bor) < g.cfg.Borrowers {
		p := prefixes[g.rng.Intn(len(prefixes))]
		b := businesses[g.rng.Intn(len(businesses))]
		base := p + " " + b.name
		if used[base] {
			continue
		}
		used[base] = true
		bw := &borrower{base: base, sector: b.sector}
		if g.rng.Float64() < 0.08 {
			bw.alias = prefixes[g.rng.Intn(len(prefixes))] + "works"
		}
		bw.startQ = g.rng.Intn(nq+8) - 8
		bw.endQ = bw.startQ + 8 + g.rng.Intn(13)
		bw.spreadBp = 450 + 25*g.rng.Intn(13)
		bw.jrSpreadBp = bw.spreadBp + 300 + 25*g.rng.Intn(5)
		bw.hasJunior = g.rng.Float64() < 0.12
		bw.foreign = g.rng.Float64() < 0.03
		bw.size = math.Exp(17.6 + 1.0*g.rng.NormFloat64())
		orig := g.qEnds[0].AddDays(bw.startQ * 91)
		bw.maturity = day.New(orig.Year()+5+g.rng.Intn(3), 1+g.rng.Intn(12), 1).MonthEnd()

		v := 0.985 + 0.01*g.rng.Float64()
		bw.senior = make([]float64, n)
		bw.junior = make([]float64, n)
		shock := sectorShock[b.sector]
		for i := 0; i < n; i++ {
			if i > 0 {
				gap := math.Max(0, 0.97-v)
				beta := 0.012 + 0.10*gap
				dv := -beta * (g.baa[i] - g.baa[i-1])
				dv += (1 + 5*gap) * shock[i]
				dv += (0.0004 + 0.012*gap) * g.rng.NormFloat64()
				// A performing loan below par is pulled back to it, about a
				// seventh of the way each quarter, as real marks are. A loan
				// under stress is not: in the real schedules it is as likely
				// to fall further as to recover.
				if v >= 0.97 && v < 0.992 {
					dv += 0.0024 * (0.992 - v)
				}
				if g.rng.Float64() < 0.035/252 {
					dv -= 0.04 + 0.07*g.rng.ExpFloat64()
				}
				v += dv
				v = math.Min(1.005, math.Max(0.02, v))
			}
			bw.senior[i] = v
			bw.junior[i] = math.Min(1.005, math.Max(0.01, 1-2.2*(1-v)))
		}
		g.bor = append(g.bor, bw)
		g.truth.Core = append(g.truth.Core, resolve.Core(base))
	}
}
