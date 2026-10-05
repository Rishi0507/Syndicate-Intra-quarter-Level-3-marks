package resolve

import (
	"sort"
	"strings"
)

// TermKey fingerprints a loan by the two terms every lender in it reports the
// same way: the month it matures and its margin in basis points. Two names
// that share a fingerprint are very unlikely to be different companies.
type TermKey uint64

// MakeTermKey returns the zero key when either term is missing, so partial
// information never corroborates anything.
func MakeTermKey(maturityYYYYMM int, spreadBp int) TermKey {
	if maturityYYYYMM <= 0 || spreadBp <= 0 {
		return 0
	}
	return TermKey(uint64(maturityYYYYMM)*100000 + uint64(spreadBp))
}

type mention struct {
	name    string
	strict  string
	core    []string
	coreKey string
	aliases []string
	weight  float64
	terms   map[TermKey]struct{}
	holders map[string]struct{}
}

// Resolver accumulates every spelling of every borrower and then clusters them.
type Resolver struct {
	byName map[string]*mention
	order  []*mention
}

func New() *Resolver { return &Resolver{byName: map[string]*mention{}} }

// Add records one sighting of a borrower name by a holder. weight is the fair
// value behind it and decides which spelling a cluster is displayed under.
func (r *Resolver) Add(name string, aliases []string, weight float64, term TermKey, holder string) {
	m := r.byName[name]
	if m == nil {
		m = &mention{
			name:    name,
			strict:  Strict(name),
			core:    CoreTokens(name),
			terms:   map[TermKey]struct{}{},
			holders: map[string]struct{}{},
		}
		m.coreKey = strings.Join(m.core, " ")
		r.byName[name] = m
		r.order = append(r.order, m)
	}
	m.weight += weight
	if term != 0 {
		m.terms[term] = struct{}{}
	}
	m.holders[holder] = struct{}{}
	for _, a := range aliases {
		if !contains(m.aliases, a) {
			m.aliases = append(m.aliases, a)
		}
	}
}

func contains(xs []string, x string) bool {
	for _, v := range xs {
		if v == x {
			return true
		}
	}
	return false
}

// Cluster is one resolved borrower.
type Cluster struct {
	ID      int
	Name    string   // the spelling with the most fair value behind it
	Key     string   // normalised core name
	Members []string // every spelling merged into it
	Holders int
}

// Report counts how many merges each rule produced.
type Report struct {
	Mentions      int
	Clusters      int
	ByStrict      int
	ByCore        int
	ByCoreTerms   int
	ByAlias       int
	ByFuzzyTerms  int
	OversizeBlock int
}

type unionFind struct{ parent []int }

func newUF(n int) *unionFind {
	u := &unionFind{parent: make([]int, n)}
	for i := range u.parent {
		u.parent[i] = i
	}
	return u
}

func (u *unionFind) find(x int) int {
	for u.parent[x] != x {
		u.parent[x] = u.parent[u.parent[x]]
		x = u.parent[x]
	}
	return x
}

func (u *unionFind) union(a, b int) bool {
	ra, rb := u.find(a), u.find(b)
	if ra == rb {
		return false
	}
	if ra > rb {
		ra, rb = rb, ra
	}
	u.parent[rb] = ra
	return true
}

func sharesTerm(a, b *mention) bool {
	if len(a.terms) > len(b.terms) {
		a, b = b, a
	}
	for t := range a.terms {
		if _, ok := b.terms[t]; ok {
			return true
		}
	}
	return false
}

// similar reports whether two core names are close enough to be the same
// company given independent corroboration: one contains the other, they
// overlap heavily, or they differ by a typo.
func similar(a, b []string) bool {
	if len(a) == 0 || len(b) == 0 {
		return false
	}
	inB := make(map[string]bool, len(b))
	for _, t := range b {
		inB[t] = true
	}
	shared := 0
	for _, t := range a {
		if inB[t] {
			shared++
		}
	}
	small, short := len(a), a
	if len(b) < small {
		small, short = len(b), b
	}
	if shared == small {
		// Subset: "acme software" within "acme software solution group". A
		// single shared word counts only if it is distinctive.
		if small >= 2 {
			return true
		}
		return len(short[0]) >= 4 && !generic[short[0]] && !isSmallInt(short[0])
	}
	union := len(a) + len(b) - shared
	if union > 0 && float64(shared)/float64(union) >= 0.6 {
		return true
	}
	ja, jb := strings.Join(a, ""), strings.Join(b, "")
	limit := 1
	if len(ja) >= 10 && len(jb) >= 10 {
		limit = 2
	}
	return editDistance(ja, jb, limit) <= limit
}

// editDistance is Levenshtein distance, abandoning early once it exceeds max.
func editDistance(a, b string, max int) int {
	if d := len(a) - len(b); d > max || -d > max {
		return max + 1
	}
	prev := make([]int, len(b)+1)
	cur := make([]int, len(b)+1)
	for j := range prev {
		prev[j] = j
	}
	for i := 1; i <= len(a); i++ {
		cur[0] = i
		rowMin := cur[0]
		for j := 1; j <= len(b); j++ {
			cost := 1
			if a[i-1] == b[j-1] {
				cost = 0
			}
			v := prev[j-1] + cost
			if prev[j]+1 < v {
				v = prev[j] + 1
			}
			if cur[j-1]+1 < v {
				v = cur[j-1] + 1
			}
			cur[j] = v
			if v < rowMin {
				rowMin = v
			}
		}
		if rowMin > max {
			return max + 1
		}
		prev, cur = cur, prev
	}
	return prev[len(b)]
}

// maxBlock bounds the quadratic fuzzy pass. A block this large is a maturity
// and margin so common that sharing it says little.
const maxBlock = 900

// generic words are too common in company names for one of them alone to
// tie two names together.
var generic = set("group", "capital", "health", "care", "service", "solution", "system",
	"technology", "partner", "international", "global", "american", "national", "us",
	"management", "financial", "medical", "software", "industry", "product", "enterprise",
	"network", "resource", "energy", "media", "consulting", "project", "investment", "holding")
