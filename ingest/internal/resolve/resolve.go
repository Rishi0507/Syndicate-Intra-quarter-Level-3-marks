package resolve

import (
	"sort"
	"strings"
)

type TermKey uint64

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

type Resolver struct {
	byName map[string]*mention
	order  []*mention
}

func New() *Resolver { return &Resolver{byName: map[string]*mention{}} }

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

type Cluster struct {
	ID      int
	Name    string
	Key     string
	Members []string
	Holders int
}

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

const maxBlock = 900

var generic = set("group", "capital", "health", "care", "service", "solution", "system",
	"technology", "partner", "international", "global", "american", "national", "us",
	"management", "financial", "medical", "software", "industry", "product", "enterprise",
	"network", "resource", "energy", "media", "consulting", "project", "investment", "holding")

func (r *Resolver) Resolve() (map[string]int, []Cluster, Report) {
	ms := r.order
	uf := newUF(len(ms))
	rep := Report{Mentions: len(ms)}

	first := map[string]int{}
	for i, m := range ms {
		if m.strict == "" {
			continue
		}
		if j, ok := first[m.strict]; ok {
			if uf.union(i, j) {
				rep.ByStrict++
			}
		} else {
			first[m.strict] = i
		}
	}

	groups := map[string][]int{}
	for i, m := range ms {
		if m.coreKey != "" {
			groups[m.coreKey] = append(groups[m.coreKey], i)
		}
	}
	for _, g := range groups {
		if len(g) < 2 {
			continue
		}
		if len(ms[g[0]].core) >= 2 {
			for _, i := range g[1:] {
				if uf.union(g[0], i) {
					rep.ByCore++
				}
			}
			continue
		}
		byTerm := map[TermKey]int{}
		for _, i := range g {
			for t := range ms[i].terms {
				if j, ok := byTerm[t]; ok {
					if uf.union(i, j) {
						rep.ByCoreTerms++
					}
				} else {
					byTerm[t] = i
				}
			}
		}
	}

	for i, m := range ms {
		for _, a := range m.aliases {
			toks := CoreTokens(a)
			g, ok := groups[strings.Join(toks, " ")]
			if !ok {
				continue
			}
			for _, j := range g {
				if len(toks) >= 2 || sharesTerm(m, ms[j]) {
					if uf.union(i, j) {
						rep.ByAlias++
					}
				}
			}
		}
	}

	blocks := map[TermKey][]int{}
	for i, m := range ms {
		for t := range m.terms {
			blocks[t] = append(blocks[t], i)
		}
	}
	keys := make([]TermKey, 0, len(blocks))
	for t := range blocks {
		keys = append(keys, t)
	}
	sort.Slice(keys, func(i, j int) bool { return keys[i] < keys[j] })
	for _, t := range keys {
		b := blocks[t]
		if len(b) > maxBlock {
			rep.OversizeBlock++
			continue
		}
		sort.Ints(b)
		for x := 0; x < len(b); x++ {
			for y := x + 1; y < len(b); y++ {
				i, j := b[x], b[y]
				if uf.find(i) == uf.find(j) {
					continue
				}
				if similar(ms[i].core, ms[j].core) {
					if uf.union(i, j) {
						rep.ByFuzzyTerms++
					}
				}
			}
		}
	}

	rootID := map[int]int{}
	var clusters []Cluster
	best := []float64{}
	holders := []map[string]struct{}{}
	assign := make(map[string]int, len(ms))
	for i, m := range ms {
		root := uf.find(i)
		id, ok := rootID[root]
		if !ok {
			id = len(clusters)
			rootID[root] = id
			clusters = append(clusters, Cluster{ID: id})
			best = append(best, -1)
			holders = append(holders, map[string]struct{}{})
		}
		c := &clusters[id]
		c.Members = append(c.Members, m.name)
		if m.weight > best[id] {
			best[id] = m.weight
			c.Name = m.name
			c.Key = m.coreKey
		}
		for h := range m.holders {
			holders[id][h] = struct{}{}
		}
		assign[m.name] = id
	}
	for i := range clusters {
		sort.Strings(clusters[i].Members)
		clusters[i].Holders = len(holders[i])
	}
	rep.Clusters = len(clusters)
	return assign, clusters, rep
}
