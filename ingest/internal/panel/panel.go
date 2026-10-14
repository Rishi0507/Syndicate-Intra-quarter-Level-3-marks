package panel

import (
	"fmt"
	"math"
	"regexp"
	"sort"
	"strings"

	"syndicate/ingest/internal/day"
	"syndicate/ingest/internal/ident"
	"syndicate/ingest/internal/resolve"
	"syndicate/ingest/internal/soi"
	"syndicate/ingest/internal/taxonomy"
)

type Class uint8

const (
	Senior Class = iota
	Junior
	Unclassified
)

var classNames = [...]string{"senior", "junior", "unclassified"}

func (c Class) String() string { return classNames[c] }

func classOf(l ident.Lien) Class {
	switch l {
	case ident.FirstLien, ident.Unitranche:
		return Senior
	case ident.SecondLien, ident.Subordinated:
		return Junior
	}
	return Unclassified
}

const (
	minPrice = 0.0
	maxPrice = 1.04
)

const (
	minBasis = 0.85
	maxBasis = 1.05
)

const firstDate = day.Date(20220901)

const spreadBucketBp = 25

type position struct {
	holder    int32
	mention   int32
	asof      day.Date
	filed     day.Date
	original  bool
	lien      ident.Lien
	facility  ident.Facility
	maturity  day.Date
	spreadBp  int32
	principal float64
	cost      float64
	fv        float64
	sector    int8
}

type Holder struct {
	ID     int
	CIK    string
	Name   string
	Family string
	First  day.Date
	Last   day.Date
	Marks  int
}

type Borrower struct {
	ID        int
	Name      string
	Key       string
	Sector    string
	Spellings int
	Holders   int
	Families  int
}

type Instrument struct {
	ID       int
	Borrower int
	Class    Class
	Piece    int
	Holders  int
	Families int
	Marks    int
	First    day.Date
	Last     day.Date
}

type Mark struct {
	Instrument int
	Holder     int
	AsOf       day.Date
	Filed      day.Date
	Original   bool
	Price      float64
	Principal  float64
	FairValue  float64
	Cost       float64
	SpreadBp   float64
	Maturity   day.Date
	Rows       int
}

type TrancheMark struct {
	Borrower   int
	Class      Class
	MaturityYM int
	SpreadBp   int
	Holder     int
	AsOf       day.Date
	Price      float64
	Principal  float64
}

type Report struct {
	Archives        int            `json:"archives"`
	RowsRead        int            `json:"rows_read"`
	RowsSuperseded  int            `json:"rows_superseded"`
	RowsDuplicate   int            `json:"rows_duplicate_filing"`
	RowsKept        int            `json:"rows_kept"`
	Dropped         map[string]int `json:"dropped"`
	Pieces          map[string]int `json:"pieces"`
	Holders         int            `json:"holders"`
	Families        int            `json:"families"`
	Borrowers       int            `json:"borrowers"`
	Instruments     int            `json:"instruments"`
	Marks           int            `json:"marks"`
	TrancheMarks    int            `json:"tranche_marks"`
	MultiHolder     int            `json:"instruments_multi_holder"`
	MultiFamily     int            `json:"instruments_multi_family"`
	ClassInferred   int            `json:"class_inferred_from_terms"`
	FirstAsOf       string         `json:"first_asof"`
	LastAsOf        string         `json:"last_asof"`
	Resolve         resolve.Report `json:"resolve"`
	UnmappedSectors []string       `json:"unmapped_sector_labels_sample"`
}

type Panel struct {
	Holders     []Holder
	Borrowers   []Borrower
	Instruments []Instrument
	Marks       []Mark
	Tranches    []TrancheMark
	Report      Report
}

var foreignRe = regexp.MustCompile(`(?i)\b(eur|gbp|cad|aud|sek|nok|dkk|chf|jpy|nzd|euribor|sonia|cdor|corra|bbsy|bbsw|stibor|nibor|saron|tibor)\b|[€£]|\bc\$|\ba\$` +
	`|\b(british pound|pound sterling|australian dollar|canadian dollar|new zealand dollar|swedish krona|norwegian krone|danish krone|swiss franc|japanese yen)\b` +
	`|\b(sn|e|c|b)\s*\+\s*\d`)

var europeanFormRe = regexp.MustCompile(`(?i)\b(oy|oyj|ab|as|asa|aps|s\.?p\.?a\.?|s\.?r\.?l\.?|s\.?a\.?s\.?|s\.?a\.?r\.?l\.?|s\.?à\s*r\.?l\.?|gmbh|b\.?v\.?|n\.?v\.?|ag|s\.?l\.?|kft|sp\.? z o\.?o\.?)\s*$`)

func foreignRate(member string) bool {
	m := strings.ToLower(member)
	if m == "" {
		return false
	}
	for _, usd := range []string{"sofr", "prime", "federalfunds", "fedfunds", "libor", "treasury", "fixed", "bsby"} {
		if strings.Contains(m, usd) && !strings.Contains(m, "euro") && !strings.Contains(m, "sterling") {
			return false
		}
	}
	for _, other := range []string{"euribor", "sonia", "corra", "cdor", "bbsy", "bbsw", "stibor", "nibor", "saron", "tibor", "cibor", "bkbm"} {
		if strings.Contains(m, other) {
			return true
		}
	}
	return false
}

func foreignAxis(member string) bool {
	m := strings.ToLower(member)
	if m == "" || strings.Contains(m, "usd") || strings.Contains(m, "united states") || strings.Contains(m, "u.s. dollar") {
		return false
	}
	return true
}

type groupKey struct {
	cik  string
	asof day.Date
}

type filing struct {
	adsh     string
	filed    day.Date
	original bool
}

func (f filing) betterThan(g filing) bool {
	if g.adsh == "" {
		return true
	}
	if f.original != g.original {
		return f.original
	}
	if f.filed != g.filed {
		return f.filed < g.filed
	}
	return f.adsh < g.adsh
}

func normaliseSpread(v float64) int32 {
	switch {
	case v <= 0:
		return 0
	case v < 0.25:
		v *= 10000
	case v <= 25:
		v *= 100
	}
	if v > 2500 {
		return 0
	}
	return int32(math.Round(v))
}

func bucket(bp int32) int32 {
	return (bp + spreadBucketBp/2) / spreadBucketBp * spreadBucketBp
}

func Build(archives []string, logf func(string, ...any)) (*Panel, error) {
	rep := Report{Archives: len(archives), Dropped: map[string]int{}, Pieces: map[string]int{}}

	chosen := map[groupKey]filing{}
	owner := map[string]int{}
	for ai, a := range archives {
		st, err := soi.ReadZip(a, func(r *soi.Row) error {
			if r.CIK == "" || r.AsOf.IsZero() || r.Adsh == "" {
				return nil
			}
			if o, seen := owner[r.Adsh]; !seen {
				owner[r.Adsh] = ai
			} else if o != ai {
				return nil
			}
			k := groupKey{r.CIK, r.AsOf}
			f := filing{r.Adsh, r.Filed, r.Period == r.AsOf}
			if f.betterThan(chosen[k]) {
				chosen[k] = f
			}
			return nil
		})
		if err != nil {
			return nil, fmt.Errorf("%s: %w", a, err)
		}
		rep.RowsRead += st.Rows
		logf("scanned %s: %d rows", a, st.Rows)
	}

	var (
		positions  []position
		holders    []Holder
		holderIdx  = map[string]int32{}
		mentions   []string
		mentionIdx = map[string]int32{}
		res        = resolve.New()
		sectorCode = map[string]int8{}
		unmapped   = map[string]int{}
	)
	for i, s := range taxonomy.Sectors {
		sectorCode[s.Code] = int8(i)
	}
	drop := func(why string) { rep.Dropped[why]++ }
	type holding struct {
		cik      string
		borrower string
	}
	tainted := map[holding]bool{}

	for ai, a := range archives {
		_, err := soi.ReadZip(a, func(r *soi.Row) error {
			if r.CIK == "" || r.AsOf.IsZero() || r.Adsh == "" {
				drop("no holder or date")
				return nil
			}
			if owner[r.Adsh] != ai {
				rep.RowsDuplicate++
				return nil
			}
			if r.AsOf < firstDate {
				rep.RowsSuperseded++
				return nil
			}
			f := chosen[groupKey{r.CIK, r.AsOf}]
			if f.adsh != r.Adsh {
				rep.RowsSuperseded++
				return nil
			}
			if r.Identifier == "" {
				drop("no identifier")
				return nil
			}
			if !r.HasFairValue {
				drop("no fair value")
				return nil
			}
			p := ident.ParseFor(r.Identifier, r.Type, r.Holder)
			if !p.Lien.IsDebt() || (r.Shares > 0 && !r.HasPrincipal) {
				drop("equity or structured")
				return nil
			}
			if p.Unfunded {
				drop("unfunded commitment")
				return nil
			}
			if r.Entity != "" {
				drop("inside a subsidiary")
				return nil
			}
			if issuer := ident.TrimLabels(strings.TrimSuffix(strings.TrimSpace(r.Issuer), "[Member]")); issuer != "" &&
				len(issuer) <= 70 && !strings.Contains(issuer, "%") && !(ident.Parsed{Borrower: issuer}).Nameless() {
				p.Borrower = issuer
			}
			if !r.HasPrincipal || r.Principal <= 0 {
				drop("no principal")
				return nil
			}
			if p.Nameless() {
				drop("no borrower name")
				return nil
			}
			if foreignRe.MatchString(p.Descriptor) || foreignAxis(r.Currency) || foreignRate(r.RateType) {
				tainted[holding{r.CIK, p.Borrower}] = true
				drop("non-dollar")
				return nil
			}
			if r.HasCost && r.Cost > 850*r.Principal && r.Cost < 1050*r.Principal {
				r.Principal *= 1000
			}
			if r.Principal >= 5e9 {
				r.Principal /= 1000
				r.FairValue /= 1000
				r.Cost /= 1000
			}
			if r.HasCost {
				basis := r.Cost / r.Principal
				switch {
				case basis < 0.5:
					drop("unfunded commitment")
					return nil
				case basis < minBasis || basis > maxBasis:
					if basis > maxBasis {
						tainted[holding{r.CIK, p.Borrower}] = true
					}
					drop("cost not that of a dollar loan at par")
					return nil
				case basis > 1.01 && europeanFormRe.MatchString(p.Borrower):
					tainted[holding{r.CIK, p.Borrower}] = true
					drop("non-dollar")
					return nil
				}
			}
			price := r.FairValue / r.Principal
			if price < minPrice || price > maxPrice {
				if price > maxPrice {
					tainted[holding{r.CIK, p.Borrower}] = true
				}
				drop("price out of range")
				return nil
			}
			if !r.HasCost && (price < 0.05 || (price < 0.5 && p.Facility != ident.Term)) {
				drop("unfunded commitment")
				return nil
			}

			hi, ok := holderIdx[r.CIK]
			if !ok {
				hi = int32(len(holders))
				holderIdx[r.CIK] = hi
				holders = append(holders, Holder{ID: int(hi), CIK: r.CIK, Name: r.Holder,
					Family: taxonomy.FamilyOf(r.Holder), First: r.AsOf, Last: r.AsOf})
			}
			h := &holders[hi]
			if r.AsOf < h.First {
				h.First = r.AsOf
			}
			if r.AsOf >= h.Last {
				h.Last = r.AsOf
				h.Name = r.Holder
			}

			mi, ok := mentionIdx[p.Borrower]
			if !ok {
				mi = int32(len(mentions))
				mentionIdx[p.Borrower] = mi
				mentions = append(mentions, p.Borrower)
			}

			bp := int32(0)
			if r.HasSpread {
				bp = normaliseSpread(r.Spread)
			}
			ym := 0
			if !r.Maturity.IsZero() {
				ym = r.Maturity.Year()*100 + r.Maturity.Month()
			}
			res.Add(p.Borrower, p.Aliases, r.FairValue, resolve.MakeTermKey(ym, int(bucket(bp))), r.CIK)

			sec := int8(-1)
			label := r.Sector
			if strings.TrimSpace(label) == "" {
				label = p.Industry
			}
			if code := taxonomy.SectorOf(label); code != "" {
				sec = sectorCode[code]
				if code == "other" {
					unmapped[label]++
				}
			}
			cost := 0.0
			if r.HasCost {
				cost = r.Cost
			}
			positions = append(positions, position{
				holder: hi, mention: mi, asof: r.AsOf, filed: f.filed, original: f.original,
				lien: p.Lien, facility: p.Facility, maturity: r.Maturity, spreadBp: bp,
				principal: r.Principal, cost: cost, fv: r.FairValue, sector: sec,
			})
			return nil
		})
		if err != nil {
			return nil, fmt.Errorf("%s: %w", a, err)
		}
		logf("kept %d positions after %s", len(positions), a)
	}
	if len(tainted) > 0 {
		kept := positions[:0]
		for _, p := range positions {
			if tainted[holding{holders[p.holder].CIK, mentions[p.mention]}] {
				rep.Dropped["non-dollar on another date"]++
				continue
			}
			kept = append(kept, p)
		}
		positions = kept
	}
	rep.RowsKept = len(positions)

	assign, clusters, rrep := res.Resolve()
	rep.Resolve = rrep
	borrowerOf := make([]int, len(mentions))
	for i, m := range mentions {
		borrowerOf[i] = assign[m]
	}

	type termID struct {
		borrower int
		ym       int
		bp       int32
	}
	known := map[termID]Class{}
	for i := range positions {
		p := &positions[i]
		c := classOf(p.lien)
		if c == Unclassified || p.maturity.IsZero() || p.spreadBp == 0 {
			continue
		}
		k := termID{borrowerOf[p.mention], p.maturity.Year()*100 + p.maturity.Month(), bucket(p.spreadBp)}
		if prev, ok := known[k]; !ok || c < prev {
			known[k] = c
		}
	}
	type margin struct {
		sum, weight float64
	}
	byClass := map[[2]int]*margin{}
	for i := range positions {
		p := &positions[i]
		c := classOf(p.lien)
		if c == Unclassified {
			continue
		}
		k := [2]int{borrowerOf[p.mention], int(c)}
		m := byClass[k]
		if m == nil {
			m = &margin{}
			byClass[k] = m
		}
		if p.spreadBp > 0 {
			m.sum += float64(p.spreadBp) * p.principal
			m.weight += p.principal
		}
	}
	const marginReach = 150.0
	classes := make([]Class, len(positions))
	for i := range positions {
		p := &positions[i]
		c := classOf(p.lien)
		if c == Unclassified {
			b := borrowerOf[p.mention]
			if !p.maturity.IsZero() && p.spreadBp != 0 {
				k := termID{b, p.maturity.Year()*100 + p.maturity.Month(), bucket(p.spreadBp)}
				if kc, ok := known[k]; ok {
					c = kc
				}
			}
			if c == Unclassified {
				sen, jun := byClass[[2]int{b, int(Senior)}], byClass[[2]int{b, int(Junior)}]
				switch {
				case sen != nil && jun == nil:
					c = Senior
				case sen == nil && jun != nil:
				case sen != nil && jun != nil && p.spreadBp > 0 && sen.weight > 0 && jun.weight > 0:
					ds := math.Abs(float64(p.spreadBp) - sen.sum/sen.weight)
					dj := math.Abs(float64(p.spreadBp) - jun.sum/jun.weight)
					if ds <= dj && ds <= marginReach {
						c = Senior
					} else if dj < ds && dj <= marginReach {
						c = Junior
					}
				}
			}
			if c != Unclassified {
				rep.ClassInferred++
			}
		}
		classes[i] = c
	}

	type instKey struct {
		borrower int
		class    Class
	}
	type markKey struct {
		inst   int
		holder int32
		asof   day.Date
	}
	type agg struct {
		principal, fv, cost, spreadW, spreadP float64
		maturity, filed                       day.Date
		original                              bool
		rows                                  int
	}
	instIdx := map[instKey]int{}
	var insts []Instrument
	marks := map[markKey]*agg{}
	var markOrder []markKey

	type trKey struct {
		borrower int
		class    Class
		ym       int
		bp       int32
		holder   int32
		asof     day.Date
	}
	tranches := map[trKey]*[2]float64{}
	var trOrder []trKey

	sectorVotes := make([][]float64, len(clusters))
	for i := range sectorVotes {
		sectorVotes[i] = make([]float64, len(taxonomy.Sectors))
	}

	for i := range positions {
		p := &positions[i]
		b := borrowerOf[p.mention]
		c := classes[i]
		if p.sector >= 0 {
			sectorVotes[b][p.sector] += p.fv + 1
		}
		ik := instKey{b, c}
		ii, ok := instIdx[ik]
		if !ok {
			ii = len(insts)
			instIdx[ik] = ii
			insts = append(insts, Instrument{ID: ii, Borrower: b, Class: c, First: p.asof, Last: p.asof})
		}
		mk := markKey{ii, p.holder, p.asof}
		a := marks[mk]
		if a == nil {
			a = &agg{filed: p.filed, original: p.original}
			marks[mk] = a
			markOrder = append(markOrder, mk)
		}
		a.principal += p.principal
		a.fv += p.fv
		a.cost += p.cost
		if p.spreadBp > 0 {
			a.spreadW += float64(p.spreadBp) * p.principal
			a.spreadP += p.principal
		}
		if p.maturity > a.maturity {
			a.maturity = p.maturity
		}
		a.rows++

		if p.facility == ident.Term && !p.maturity.IsZero() && p.spreadBp > 0 && c != Unclassified {
			tk := trKey{b, c, p.maturity.Year()*100 + p.maturity.Month(), bucket(p.spreadBp), p.holder, p.asof}
			t := tranches[tk]
			if t == nil {
				t = &[2]float64{}
				tranches[tk] = t
				trOrder = append(trOrder, tk)
			}
			t[0] += p.principal
			t[1] += p.fv
		}
	}
	positions = nil

	sort.Slice(markOrder, func(i, j int) bool {
		a, b := markOrder[i], markOrder[j]
		if a.inst != b.inst {
			return a.inst < b.inst
		}
		if a.asof != b.asof {
			return a.asof < b.asof
		}
		return a.holder < b.holder
	})

	{
		type seriesKey struct {
			inst   int
			holder int32
		}
		bySeries := map[seriesKey][]markKey{}
		for _, mk := range markOrder {
			k := seriesKey{mk.inst, mk.holder}
			bySeries[k] = append(bySeries[k], mk)
		}
		glitch := map[markKey]bool{}
		price := func(mk markKey) float64 { a := marks[mk]; return a.fv / a.principal }
		for _, s := range bySeries {
			for i := 1; i+1 < len(s); i++ {
				before, here, after := price(s[i-1]), price(s[i]), price(s[i+1])
				if math.Abs(before-after) <= 0.05 && math.Abs(here-before) >= 0.20 && (here-before)*(here-after) > 0 {
					glitch[s[i]] = true
				}
			}
			if len(s) >= 3 {
				first, next, then := price(s[0]), price(s[1]), price(s[2])
				if math.Abs(next-then) <= 0.05 && next-first >= 0.20 {
					glitch[s[0]] = true
				}
			}
		}
		if len(glitch) > 0 {
			kept := markOrder[:0]
			for _, mk := range markOrder {
				if glitch[mk] {
					rep.Dropped["reporting glitch"]++
					continue
				}
				kept = append(kept, mk)
			}
			markOrder = kept
		}
	}

	{
		price := func(mk markKey) float64 { a := marks[mk]; return a.fv / a.principal }
		pieces := map[int]int{}
		origin := map[int]int{}
		newPiece := func(from int) int {
			root := from
			if o, ok := origin[from]; ok {
				root = o
			}
			pieces[root]++
			id := len(insts)
			insts = append(insts, Instrument{ID: id, Borrower: insts[root].Borrower, Class: insts[root].Class, Piece: pieces[root]})
			origin[id] = root
			return id
		}
		move := func(mk markKey, to int) markKey {
			a := marks[mk]
			delete(marks, mk)
			mk.inst = to
			marks[mk] = a
			return mk
		}

		type seriesKey struct {
			inst   int
			holder int32
		}
		type restart struct {
			inst int
			asof day.Date
		}
		series := map[seriesKey][]int{}
		var seriesOrder []seriesKey
		for i, mk := range markOrder {
			k := seriesKey{mk.inst, mk.holder}
			if series[k] == nil {
				seriesOrder = append(seriesOrder, k)
			}
			series[k] = append(series[k], i)
		}
		restarts := map[restart]int{}
		for _, k := range seriesOrder {
			s := series[k]
			mk := markOrder[s[0]]
			inst := mk.inst
			for j := 1; j < len(s); j++ {
				before, here := price(markOrder[s[j-1]]), price(markOrder[s[j]])
				if here-before >= 0.25 && here >= 0.95 {
					rk := restart{inst, markOrder[s[j]].asof}
					to, ok := restarts[rk]
					if !ok {
						to = newPiece(inst)
						restarts[rk] = to
						rep.Pieces["new paper after a restructuring"]++
					}
					inst = to
				}
				if inst != mk.inst {
					markOrder[s[j]] = move(markOrder[s[j]], inst)
				}
			}
		}

		type famDate struct {
			family string
			asof   day.Date
		}
		byInst := map[int]map[famDate]*[2]float64{}
		for _, mk := range markOrder {
			a := marks[mk]
			m := byInst[mk.inst]
			if m == nil {
				m = map[famDate]*[2]float64{}
				byInst[mk.inst] = m
			}
			k := famDate{holders[mk.holder].Family, mk.asof}
			if m[k] == nil {
				m[k] = &[2]float64{}
			}
			m[k][0] += a.principal
			m[k][1] += a.fv
		}
		moved := map[seriesKey]int{}
		n0 := len(insts)
		for inst := 0; inst < n0; inst++ {
			cells := byInst[inst]
			dates := map[string][]day.Date{}
			for k := range cells {
				dates[k.family] = append(dates[k.family], k.asof)
			}
			if len(dates) < 2 {
				continue
			}
			families := make([]string, 0, len(dates))
			for f := range dates {
				families = append(families, f)
			}
			sort.Slice(families, func(i, j int) bool {
				if len(dates[families[i]]) != len(dates[families[j]]) {
					return len(dates[families[i]]) > len(dates[families[j]])
				}
				return families[i] < families[j]
			})
			apart := func(a, b string) bool {
				common, far := 0, 0
				for _, d := range dates[a] {
					cb := cells[famDate{b, d}]
					if cb == nil {
						continue
					}
					ca := cells[famDate{a, d}]
					common++
					if math.Abs(ca[1]/ca[0]-cb[1]/cb[0]) >= 0.10 {
						far++
					}
				}
				return common >= 4 && 5*far >= 4*common
			}
			var groups [][]string
			group := map[string]int{}
			for _, f := range families {
				placed := false
				for gi, members := range groups {
					fits := true
					for _, m := range members {
						if apart(f, m) {
							fits = false
							break
						}
					}
					if fits {
						groups[gi] = append(members, f)
						group[f] = gi
						placed = true
						break
					}
				}
				if !placed {
					group[f] = len(groups)
					groups = append(groups, []string{f})
				}
			}
			if len(groups) < 2 {
				continue
			}
			to := make([]int, len(groups))
			to[0] = inst
			for gi := 1; gi < len(groups); gi++ {
				to[gi] = newPiece(inst)
				rep.Pieces["a different tranche"]++
			}
			for f, gi := range group {
				if gi > 0 {
					for i := range holders {
						if holders[i].Family == f {
							moved[seriesKey{inst, int32(i)}] = to[gi]
						}
					}
				}
			}
		}
		if len(moved) > 0 {
			for i, mk := range markOrder {
				if to, ok := moved[seriesKey{mk.inst, mk.holder}]; ok {
					markOrder[i] = move(mk, to)
				}
			}
		}
		if len(insts) > n0 || len(restarts) > 0 {
			sort.Slice(markOrder, func(i, j int) bool {
				a, b := markOrder[i], markOrder[j]
				if a.inst != b.inst {
					return a.inst < b.inst
				}
				if a.asof != b.asof {
					return a.asof < b.asof
				}
				return a.holder < b.holder
			})
		}
		for i := range insts {
			insts[i].First, insts[i].Last = math.MaxInt32, 0
		}
	}

	instHolders := make([]map[int32]struct{}, len(insts))
	instFamilies := make([]map[string]struct{}, len(insts))
	borHolders := make([]map[int32]struct{}, len(clusters))
	borFamilies := make([]map[string]struct{}, len(clusters))
	out := make([]Mark, 0, len(markOrder))
	first, last := day.Date(0), day.Date(0)
	for _, mk := range markOrder {
		a := marks[mk]
		if a.principal <= 0 {
			continue
		}
		m := Mark{
			Instrument: mk.inst, Holder: int(mk.holder), AsOf: mk.asof, Filed: a.filed,
			Original: a.original, Price: a.fv / a.principal, Principal: a.principal,
			FairValue: a.fv, Cost: a.cost, Maturity: a.maturity, Rows: a.rows,
		}
		if a.spreadP > 0 {
			m.SpreadBp = a.spreadW / a.spreadP
		}
		out = append(out, m)

		in := &insts[mk.inst]
		in.Marks++
		if mk.asof < in.First {
			in.First = mk.asof
		}
		if mk.asof > in.Last {
			in.Last = mk.asof
		}
		fam := holders[mk.holder].Family
		add(&instHolders[mk.inst], mk.holder)
		addS(&instFamilies[mk.inst], fam)
		add(&borHolders[in.Borrower], mk.holder)
		addS(&borFamilies[in.Borrower], fam)
		holders[mk.holder].Marks++
		if first.IsZero() || mk.asof < first {
			first = mk.asof
		}
		if mk.asof > last {
			last = mk.asof
		}
	}

	for i := range insts {
		insts[i].Holders = len(instHolders[i])
		insts[i].Families = len(instFamilies[i])
		if insts[i].Holders > 1 {
			rep.MultiHolder++
		}
		if insts[i].Families > 1 {
			rep.MultiFamily++
		}
	}

	borrowers := make([]Borrower, len(clusters))
	for i, c := range clusters {
		sec, bestV := "", 0.0
		for j, v := range sectorVotes[i] {
			if v > bestV && taxonomy.Sectors[j].Code != "other" {
				bestV, sec = v, taxonomy.Sectors[j].Code
			}
		}
		if sec == "" {
			sec = taxonomy.SectorOfName(c.Name)
		}
		if sec == "" {
			sec = "other"
		}
		borrowers[i] = Borrower{ID: i, Name: c.Name, Key: c.Key, Sector: sec, Spellings: len(c.Members),
			Holders: len(borHolders[i]), Families: len(borFamilies[i])}
	}

	sort.Slice(trOrder, func(i, j int) bool {
		a, b := trOrder[i], trOrder[j]
		switch {
		case a.borrower != b.borrower:
			return a.borrower < b.borrower
		case a.class != b.class:
			return a.class < b.class
		case a.ym != b.ym:
			return a.ym < b.ym
		case a.bp != b.bp:
			return a.bp < b.bp
		case a.asof != b.asof:
			return a.asof < b.asof
		}
		return a.holder < b.holder
	})
	tr := make([]TrancheMark, 0, len(trOrder))
	for _, tk := range trOrder {
		t := tranches[tk]
		if t[0] <= 0 {
			continue
		}
		tr = append(tr, TrancheMark{Borrower: tk.borrower, Class: tk.class, MaturityYM: tk.ym,
			SpreadBp: int(tk.bp), Holder: int(tk.holder), AsOf: tk.asof, Price: t[1] / t[0], Principal: t[0]})
	}

	fams := map[string]struct{}{}
	for _, h := range holders {
		fams[h.Family] = struct{}{}
	}
	rep.Holders, rep.Families = len(holders), len(fams)
	rep.Borrowers, rep.Instruments = len(borrowers), len(insts)
	rep.Marks, rep.TrancheMarks = len(out), len(tr)
	rep.FirstAsOf, rep.LastAsOf = first.String(), last.String()
	rep.UnmappedSectors = topKeys(unmapped, 40)

	return &Panel{Holders: holders, Borrowers: borrowers, Instruments: insts, Marks: out, Tranches: tr, Report: rep}, nil
}

func add(m *map[int32]struct{}, k int32) {
	if *m == nil {
		*m = map[int32]struct{}{}
	}
	(*m)[k] = struct{}{}
}

func addS(m *map[string]struct{}, k string) {
	if *m == nil {
		*m = map[string]struct{}{}
	}
	(*m)[k] = struct{}{}
}

func topKeys(m map[string]int, n int) []string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Slice(keys, func(i, j int) bool {
		if m[keys[i]] != m[keys[j]] {
			return m[keys[i]] > m[keys[j]]
		}
		return keys[i] < keys[j]
	})
	if len(keys) > n {
		keys = keys[:n]
	}
	for i, k := range keys {
		keys[i] = strings.TrimSpace(k)
	}
	return keys
}
