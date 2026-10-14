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

type Config struct {
	Seed      int64
	Families  int
	Borrowers int
	Start     day.Date
	End       day.Date
}

func Default() Config {
	return Config{Seed: 20260513, Families: 34, Borrowers: 1500,
		Start: day.New(2022, 7, 1), End: day.New(2026, 9, 30)}
}

type Truth struct {
	Entity map[string]int
	Core   []string
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
	senior       []float64
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
	theta  float64
	bias   float64
	noise  float64
	style  int
	vocab  int
	types  int
	lagQ   int
	lagK   int
	fye    int
	weight float64
	funds  []*fund
	last   map[[2]int]float64
	dist   map[[2]int]float64
	spell  map[int]string
}

type fund struct {
	cik, name string
	share     float64
	holds     map[int]float64
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
	reports map[[3]int]float64
}

func isWeekday(d day.Date) bool {
	wd := d.Time().Weekday()
	return wd >= 1 && wd <= 5
}

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

func (g *gen) at(d day.Date) int {
	for i := 0; i < 7; i++ {
		if k, ok := g.dayIdx[d.AddDays(-i)]; ok {
			return k
		}
	}
	return 0
}

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

func (g *gen) makeFamilies() {
	cik := 1700000
	for i := 0; i < g.cfg.Families; i++ {
		f := &family{
			name:   familyNames[i%len(familyNames)],
			theta:  0.55 + 0.45*g.rng.Float64(),
			bias:   0.004 * g.rng.NormFloat64(),
			noise:  0.0015 + 0.0035*g.rng.Float64(),
			style:  i % 5,
			vocab:  g.rng.Intn(2),
			types:  g.rng.Intn(3),
			lagQ:   32 + g.rng.Intn(14),
			lagK:   52 + g.rng.Intn(22),
			fye:    12,
			weight: math.Exp(0.9 * g.rng.NormFloat64()),
			last:   map[[2]int]float64{},
			dist:   map[[2]int]float64{},
			spell:  map[int]string{},
		}
		if g.rng.Float64() < 0.15 {
			f.fye = []int{3, 6, 9}[g.rng.Intn(3)]
		}
		nf := 1
		if f.weight > 1.2 {
			nf = 2 + g.rng.Intn(2)
		}
		for k := 0; k < nf; k++ {
			cik += 1 + g.rng.Intn(900)
			name := strings.ToUpper(f.name + " " + vehicleKinds[k%len(vehicleKinds)])
			if k >= len(vehicleKinds) {
				name += " II"
			}
			f.funds = append(f.funds, &fund{cik: fmt.Sprint(cik), name: name,
				share: 0.4 + 0.6*g.rng.Float64(), holds: map[int]float64{}})
		}
		g.fams = append(g.fams, f)
	}
}

func (g *gen) assignHoldings() {
	total := 0.0
	for _, f := range g.fams {
		total += f.weight
	}
	pick := func() int {
		r := g.rng.Float64() * total
		for i, f := range g.fams {
			if r -= f.weight; r <= 0 {
				return i
			}
		}
		return len(g.fams) - 1
	}
	for bi, b := range g.bor {
		k := 1
		switch r := g.rng.Float64(); {
		case r < 0.52:
			k = 1
		case r < 0.74:
			k = 2
		case r < 0.86:
			k = 3
		default:
			k = 4 + g.rng.Intn(5)
		}
		chosen := map[int]bool{}
		for tries := 0; len(chosen) < k && tries < 60; tries++ {
			chosen[pick()] = true
		}
		fis := make([]int, 0, len(chosen))
		for fi := range chosen {
			fis = append(fis, fi)
		}
		sort.Ints(fis)
		for _, fi := range fis {
			f := g.fams[fi]
			slice := b.size / float64(len(fis)) * (0.5 + g.rng.Float64())
			held := false
			for _, fd := range f.funds {
				if g.rng.Float64() < 0.7 || !held && fd == f.funds[len(f.funds)-1] {
					fd.holds[bi] = math.Round(slice * fd.share)
					held = true
				}
			}
			v := vehicles[g.rng.Intn(len(vehicles))]
			s := suffixes[g.rng.Intn(len(suffixes))]
			name := b.base
			if v != "" {
				name += " " + v
			}
			if strings.HasPrefix(s, "Holdings") && strings.Contains(v, "Holdings") {
				s = "LLC"
			}
			sep := " "
			if g.rng.Float64() < 0.5 {
				sep = ", "
			}
			f.spell[bi] = name + sep + s
		}
	}
}

const (
	distCarry   = 0.8
	distWobble  = 0.13
	smoothBelow = 0.97
)

func (g *gen) mark(fi, bi, class, q int) float64 {
	key := [3]int{fi, bi, class*1000 + q}
	if m, ok := g.reports[key]; ok {
		return m
	}
	f, b := g.fams[fi], g.bor[bi]
	path := b.senior
	if class == 1 {
		path = b.junior
	}
	v := path[g.at(g.qEnds[q])]
	scale := 1 + 8*math.Max(0, 1-v)
	k := [2]int{bi, class}
	d, held := f.dist[k]
	if held {
		d = distCarry*d + math.Sqrt(1-distCarry*distCarry)*g.rng.NormFloat64()
	} else {
		d = g.rng.NormFloat64()
	}
	f.dist[k] = d
	away := math.Sqrt(1-distWobble)*d + math.Sqrt(distWobble)*g.rng.NormFloat64()
	target := v + f.bias + f.noise*scale*away
	m := target
	if prev, ok := f.last[k]; ok && (prev < smoothBelow || target < smoothBelow) {
		m = f.theta*target + (1-f.theta)*prev
	}
	m = math.Round(math.Min(1.02, math.Max(0, m))*10000) / 10000
	f.last[k] = m
	g.reports[key] = m
	return m
}

func lienText(class int, style int, rng *rand.Rand) string {
	if class == 1 {
		return []string{"Second lien senior secured loan", "Second Lien Term Loan", "Subordinated Notes"}[rng.Intn(3)]
	}
	switch style % 3 {
	case 0:
		return "First lien senior secured loan"
	case 1:
		return "Unitranche Term Loan"
	}
	return "Senior Secured Term Loan"
}

func typeAxis(class int, mode int) string {
	if mode == 2 {
		return ""
	}
	if class == 1 {
		if mode == 0 {
			return "Second Lien Debt [Member]"
		}
		return "DebtSecuritiesSecondLienMember"
	}
	if mode == 0 {
		return "First Lien Debt [Member]"
	}
	return "SeniorSecuredFirstLienLoansMember"
}

func (g *gen) identifier(f *family, bi, class, n int) string {
	b := g.bor[bi]
	name := f.spell[bi]
	lien := lienText(class, f.style+bi, g.rng)
	bp := b.spreadBp
	if class == 1 {
		bp = b.jrSpreadBp
	}
	spread := fmt.Sprintf("%.2f%%", float64(bp)/100)
	mat := fmt.Sprintf("%02d/%d", b.maturity.Month(), b.maturity.Year())
	switch f.style {
	case 0:
		return fmt.Sprintf("%s, %s", name, lien)
	case 1:
		return fmt.Sprintf("%s | %s | S + %s | %s", name, lien, spread, mat)
	case 2:
		return fmt.Sprintf("%s %s SOFR + %s due %s", strings.ReplaceAll(name, ",", ""), lien, spread, mat)
	case 3:
		return fmt.Sprintf("Debt Investments, %s, %s %d", name, lien, n)
	}
	if b.alias != "" {
		return fmt.Sprintf("%s (dba %s) (%d), %s", name, b.alias, 1+bi%9, lien)
	}
	return fmt.Sprintf("%s (%d), %s", name, 1+bi%9, lien)
}

type row struct {
	adsh, cik, holder, form   string
	asof, filed, period       day.Date
	sector, ident, affil, typ string
	rate, spread              float64
	maturity                  day.Date
	principal, cost, fv       float64
	hasPrincipal, hasRate     bool
}

func usDate(d day.Date) string { return fmt.Sprintf("%02d/%02d/%04d", d.Month(), d.Day(), d.Year()) }

const header = "adsh\tcik\tname\tddate\tform\tfiled\tperiod\tinlineurl\tcstm\t" +
	"Industry Sector Axis\tInvestment, Identifier Axis\tInvestment, Issuer Affiliation Axis\tInvestment Type Axis\t" +
	"Investment Interest Rate\tInvestment, Basis Spread, Variable Rate\tInvestment Maturity Date\t" +
	"Investment Owned, Balance, Principal Amount\tInvestment Owned, Cost\tInvestment Owned, Fair Value\t" +
	"Investment Owned, Net Assets, Percentage\tAdjusted cost basis\tInitial fair value of Investment"

func (r *row) line() string {
	num := func(v float64, ok bool, prec int) string {
		if !ok {
			return ""
		}
		return fmt.Sprintf("%.*f", prec, v)
	}
	mat := ""
	if !r.maturity.IsZero() {
		mat = r.maturity.String()
	}
	return strings.Join([]string{
		r.adsh, r.cik, r.holder, usDate(r.asof), r.form, usDate(r.filed), usDate(r.period),
		"https://example.invalid/synthetic/" + r.adsh, "2",
		r.sector, r.ident, r.affil, r.typ,
		num(r.rate, r.hasRate, 4), num(r.spread, r.hasRate, 4), mat,
		num(r.principal, r.hasPrincipal, 0), "", "",
		"", num(r.cost, true, 0), num(r.fv, true, 0),
	}, "\t")
}

func (g *gen) rowsFor(fi int, fd *fund, q int) []row {
	f := g.fams[fi]
	var out []row
	bis := make([]int, 0, len(fd.holds))
	for bi := range fd.holds {
		bis = append(bis, bi)
	}
	sort.Ints(bis)
	n := 0
	for _, bi := range bis {
		b := g.bor[bi]
		if q < b.startQ || q > b.endQ {
			continue
		}
		principal := fd.holds[bi]
		sector := sectorVocab[b.sector][f.vocab]
		n++
		add := func(class int, par float64) {
			m := g.mark(fi, bi, class, q)
			bp := b.spreadBp
			if class == 1 {
				bp = b.jrSpreadBp
			}
			id := g.identifier(f, bi, class, 1+n%3)
			g.truth.Entity[fd.cik+"|"+id] = bi
			out = append(out, row{
				sector: sector, ident: id, affil: "Unaffiliated Issuer [Member]", typ: typeAxis(class, f.types),
				rate: float64(bp)/10000 + 0.043, spread: float64(bp) / 10000, hasRate: true,
				maturity: b.maturity, principal: par, hasPrincipal: true,
				cost: math.Round(par * 0.985), fv: math.Round(par * m),
			})
		}
		if b.foreign {
			out = append(out, row{sector: sector, ident: f.spell[bi] + ", First lien senior secured loan, EUR, EURIBOR + 6.00%",
				typ: typeAxis(0, f.types), rate: 0.095, spread: 0.06, hasRate: true, maturity: b.maturity,
				principal: principal, hasPrincipal: true, cost: math.Round(principal * 1.08), fv: math.Round(principal * 1.07)})
			continue
		}
		add(0, principal)
		if b.hasJunior && bi%2 == fi%2 {
			add(1, math.Round(principal*0.3))
		}
		if bi%7 == 0 {
			out = append(out, row{sector: sector, ident: f.spell[bi] + ", Class A Units", typ: "Equity [Member]",
				cost: math.Round(principal * 0.04), fv: math.Round(principal * 0.05)})
		}
		if bi%9 == 0 {
			out = append(out, row{sector: sector, ident: f.spell[bi] + ", First lien senior secured revolving loan",
				typ: typeAxis(0, f.types), principal: 0, hasPrincipal: true, cost: 0, fv: -math.Round(principal * 0.0004)})
		}
	}
	if len(out) > 0 {
		out = append(out, row{sector: "Investment Funds & Vehicles", ident: f.name + " Senior Loan Fund LLC, Joint Venture",
			typ: "Investment Funds [Member]", cost: 5e7, fv: 4.8e7})
	}
	return out
}

func (g *gen) writeArchives(dir string) error {
	g.truth.Entity = map[string]int{}
	byQuarter := map[string][]string{}
	seq := 0
	for fi, f := range g.fams {
		for _, fd := range f.funds {
			for q, qe := range g.qEnds {
				isFYE := qe.Month() == f.fye
				form, lag := "10-Q", f.lagQ
				if isFYE {
					form, lag = "10-K", f.lagK
				}
				filed := qe.AddDays(lag + g.rng.Intn(5) - 2)
				seq++
				adsh := fmt.Sprintf("%s%s-%02d-%06d", strings.Repeat("0", 10-len(fd.cik)), fd.cik, filed.Year()%100, seq)
				emit := func(asof day.Date, rows []row) {
					for i := range rows {
						r := &rows[i]
						r.adsh, r.cik, r.holder, r.form = adsh, fd.cik, fd.name, form
						r.asof, r.filed, r.period = asof, filed, qe
						key := fmt.Sprintf("%dq%d", filed.Year(), (filed.Month()-1)/3+1)
						byQuarter[key] = append(byQuarter[key], r.line())
					}
				}
				emit(qe, g.rowsFor(fi, fd, q))
				for p := q - 1; p >= 0; p-- {
					if g.qEnds[p].Month() == f.fye {
						emit(g.qEnds[p], g.rowsFor(fi, fd, p))
						break
					}
				}
			}
		}
	}
	keys := make([]string, 0, len(byQuarter))
	for k := range byQuarter {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	for _, k := range keys {
		if err := writeZip(filepath.Join(dir, k+"_bdc.zip"), byQuarter[k]); err != nil {
			return err
		}
	}
	return nil
}

func writeZip(path string, lines []string) error {
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	zw := zip.NewWriter(f)
	w, err := zw.Create("soi.tsv")
	if err != nil {
		f.Close()
		return err
	}
	bw := bufio.NewWriter(w)
	bw.WriteString(header + "\n")
	for _, l := range lines {
		bw.WriteString(l)
		bw.WriteByte('\n')
	}
	if err := bw.Flush(); err != nil {
		f.Close()
		return err
	}
	if err := zw.Close(); err != nil {
		f.Close()
		return err
	}
	return f.Close()
}

func (g *gen) writeFactors(path string) error {
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	w := bufio.NewWriter(f)
	w.WriteString("date\tbaa_spread\thy_oas\tb_oas\tccc_oas\tbbb_oas\tvix\tsp500\tust2\tust5\tust10\tsofr\n")
	sp := 4000.0
	for i, d := range g.days {
		b := g.baa[i]
		if i > 0 {
			sp *= math.Exp(0.0003 - 0.06*(b-g.baa[i-1]) + 0.006*g.rng.NormFloat64())
		}
		fmt.Fprintf(w, "%d\t%.2f\t%.2f\t%.2f\t%.2f\t%.2f\t%.2f\t%.2f\t%.2f\t%.2f\t%.2f\t%.2f\n",
			d, b, 2.2*b-0.4, 2.6*b-0.2, 5.0*b+0.5, 0.75*b+0.1, 8+5.5*b, sp, 4.2, 4.1, 4.3, 4.3)
	}
	if err := w.Flush(); err != nil {
		f.Close()
		return err
	}
	return f.Close()
}

func (g *gen) writeTruth(dir string) error {
	f, err := os.Create(filepath.Join(dir, "truth_prices.tsv"))
	if err != nil {
		return err
	}
	w := bufio.NewWriterSize(f, 1<<20)
	w.WriteString("key\tclass\tdate\tprice\n")
	for bi, b := range g.bor {
		for i, d := range g.days {
			last := i == len(g.days)-1 || g.days[i+1].Month() != d.Month()
			if !last {
				continue
			}
			fmt.Fprintf(w, "%s\tsenior\t%d\t%.6f\n", g.truth.Core[bi], d, b.senior[i])
			if b.hasJunior {
				fmt.Fprintf(w, "%s\tjunior\t%d\t%.6f\n", g.truth.Core[bi], d, b.junior[i])
			}
		}
	}
	if err := w.Flush(); err != nil {
		f.Close()
		return err
	}
	if err := f.Close(); err != nil {
		return err
	}

	f, err = os.Create(filepath.Join(dir, "truth_families.tsv"))
	if err != nil {
		return err
	}
	w = bufio.NewWriter(f)
	w.WriteString("family\ttheta\tbias\tnoise\tfunds\n")
	for _, fm := range g.fams {
		fmt.Fprintf(w, "%s\t%.4f\t%.5f\t%.5f\t%d\n", fm.name, fm.theta, fm.bias, fm.noise, len(fm.funds))
	}
	if err := w.Flush(); err != nil {
		f.Close()
		return err
	}
	return f.Close()
}
