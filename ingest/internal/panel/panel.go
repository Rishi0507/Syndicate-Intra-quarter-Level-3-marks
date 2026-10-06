// Package panel turns raw Schedule of Investments rows into the object the
// rest of the system reasons about: a panel of marks, one per instrument per
// holder per reporting date, each stamped with the day it became public.
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

// Class is the seniority bucket marks are compared within. Lenders label the
// same loan "first lien", "unitranche" or "one stop" by house style, so those
// collapse into Senior; second lien and subordinated paper collapse into Junior.
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

// Price bounds. Floating-rate loans can be repaid at or near par, so they
// are not marked far above it. A ratio above 104 is a unit or currency
// mismatch between the principal and fair value fields, not a valuation.
const (
	minPrice = 0.0
	maxPrice = 1.04
)

// The cost of a dollar loan made near par sits close to its principal: a few
// points of original issue discount below, almost nothing above. Outside
// this band the principal is in another currency, the facility is mostly
// undrawn, or the position was bought at a deep discount. None of those
// gives a ratio that can be read as a price.
const (
	minBasis = 0.85
	maxBasis = 1.05
)

// firstDate is where the SEC's data set begins. Earlier dates appear only as
// the comparative columns of the first filings, without their own filing dates.
const firstDate = day.Date(20220901)

// Margins are reported to the basis point but lenders disagree on whether a
// credit spread adjustment is part of them. Bucketing to 25bp absorbs that.
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

// Holder is one registrant.
type Holder struct {
	ID     int
	CIK    string
	Name   string
	Family string
	First  day.Date
	Last   day.Date
	Marks  int
}

// Borrower is one resolved portfolio company.
type Borrower struct {
	ID        int
	Name      string
	Key       string
	Sector    string
	Spellings int
	Holders   int
	Families  int
}

// Instrument is a borrower's debt in one seniority class.
type Instrument struct {
	ID       int
	Borrower int
	Class    Class
	// Piece numbers the loans that one borrower and class turned out to
	// hold: zero for the first, one for the next, and so on.
	Piece    int
	Holders  int
	Families int
	Marks    int
	First    day.Date
	Last     day.Date
}

// Mark is one holder's valuation of one instrument on one reporting date.
type Mark struct {
	Instrument int
	Holder     int
	AsOf       day.Date
	Filed      day.Date
	Original   bool // carried by the filing for its own period, so Filed is the first public release
	Price      float64
	Principal  float64
	FairValue  float64
	Cost       float64
	SpreadBp   float64 // principal-weighted
	Maturity   day.Date
	Rows       int
}

// TrancheMark is a mark on a narrowly identified tranche: same borrower,
// class, maturity month and margin. It exists for the strictest version of
// the cross-holder comparison, where nothing but the valuation can differ.
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

// Report explains what happened to every row.
type Report struct {
	Archives       int            `json:"archives"`
	RowsRead       int            `json:"rows_read"`
	RowsSuperseded int            `json:"rows_superseded"`
	RowsKept       int            `json:"rows_kept"`
	Dropped        map[string]int `json:"dropped"`
	// Pieces counts the loans split out of a borrower and class that turned
	// out to hold more than one, by the sign that gave them away.
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

// Panel is the finished data set.
type Panel struct {
	Holders     []Holder
	Borrowers   []Borrower
	Instruments []Instrument
	Marks       []Mark
	Tranches    []TrancheMark
	Report      Report
}

// foreignRe spots loans denominated outside the dollar. Their principal is in
// local currency while fair value is in dollars, so the ratio is not a price.
var foreignRe = regexp.MustCompile(`(?i)\b(eur|gbp|cad|aud|sek|nok|dkk|chf|jpy|nzd|euribor|sonia|cdor|corra|bbsy|bbsw|stibor|nibor|saron|tibor)\b|[€£]|\bc\$|\ba\$` +
	`|\b(british pound|pound sterling|australian dollar|canadian dollar|new zealand dollar|swedish krona|norwegian krone|danish krone|swiss franc|japanese yen)\b` +
	`|\b(sn|e|c|b)\s*\+\s*\d`)

// europeanFormRe spots the legal forms of continental and Nordic companies.
// Such a borrower may still borrow in dollars, so this is used only together
// with a cost above par, which a dollar loan does not have.
var europeanFormRe = regexp.MustCompile(`(?i)\b(oy|oyj|ab|as|asa|aps|s\.?p\.?a\.?|s\.?r\.?l\.?|s\.?a\.?s\.?|s\.?a\.?r\.?l\.?|s\.?à\s*r\.?l\.?|gmbh|b\.?v\.?|n\.?v\.?|ag|s\.?l\.?|kft|sp\.? z o\.?o\.?)\s*$`)

// foreignRate reads the reference-rate member. Anything that is not a dollar
// rate means the loan is not in dollars.
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

// foreignAxis reads the currency axis, which names a member such as
// "EUR [Member]" or "Euro Member Countries, Euro".
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

// normaliseSpread returns a margin in basis points. XBRL percentages are
// fractions (0.0575), but some filers tag the percent (5.75) or the basis
// points (575). A loan margin lives between roughly 1% and 15%, which makes
// the three encodings distinguishable by magnitude.
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
