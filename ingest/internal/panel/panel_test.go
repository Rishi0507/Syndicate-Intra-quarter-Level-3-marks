package panel

import (
	"math"
	"path/filepath"
	"sort"
	"strings"
	"testing"

	"syndicate/ingest/internal/ident"
	"syndicate/ingest/internal/resolve"
	"syndicate/ingest/internal/soi"
	"syndicate/ingest/internal/synth"
)

func TestNormaliseSpread(t *testing.T) {
	for in, want := range map[float64]int32{0.0575: 575, 5.75: 575, 575: 575, 0: 0, -1: 0, 0.105: 1050, 9000: 0} {
		if got := normaliseSpread(in); got != want {
			t.Errorf("normaliseSpread(%v) = %d, want %d", in, got, want)
		}
	}
}

// TestSyntheticUniverse runs the whole pipeline over a generated universe and
// checks it against the answer key: every borrower recovered, no two distinct
// borrowers merged, and every non-loan row filtered out.
func TestSyntheticUniverse(t *testing.T) {
	dir := t.TempDir()
	cfg := synth.Default()
	cfg.Borrowers, cfg.Families = 500, 20
	truth, err := synth.Generate(cfg, dir)
	if err != nil {
		t.Fatal(err)
	}
	archives, err := filepath.Glob(filepath.Join(dir, "sec", "*_bdc.zip"))
	if err != nil || len(archives) == 0 {
		t.Fatalf("no archives: %v", err)
	}
	sort.Strings(archives)

	p, err := Build(archives, func(string, ...any) {})
	if err != nil {
		t.Fatal(err)
	}

	// Pairwise entity resolution quality. Re-read the rows the same way Build
	// does so each parsed borrower name can be tied back to its true identity.
	byName := map[string]int{}
	for _, b := range p.Borrowers {
		byName[b.Key] = b.ID
	}
	trueOf := map[string]map[int]bool{} // resolved core key -> true borrowers behind it
	for _, a := range archives {
		if _, err := soi.ReadZip(a, func(r *soi.Row) error {
			bi, ok := truth.Entity[r.CIK+"|"+r.Identifier]
			if !ok {
				return nil
			}
			name := ident.Parse(r.Identifier, r.Type).Borrower
			k := resolve.Core(name)
			if trueOf[k] == nil {
				trueOf[k] = map[int]bool{}
			}
			trueOf[k][bi] = true
			return nil
		}); err != nil {
			t.Fatal(err)
		}
	}
	// Every parsed name must reduce to exactly the true borrower's core key.
	wrong := 0
	for k, set := range trueOf {
		for bi := range set {
			if k != truth.Core[bi] {
				wrong++
				if wrong <= 5 {
					t.Errorf("parsed core %q for true borrower %q", k, truth.Core[bi])
				}
			}
		}
	}
	if wrong > 0 {
		t.Fatalf("%d parsed names did not reduce to their true core key", wrong)
	}

	// With names correct, clusters must be one-to-one with true borrowers.
	seen := map[string]bool{}
	for _, b := range p.Borrowers {
		if seen[b.Key] {
			t.Errorf("borrower key %q appears in two clusters (split)", b.Key)
		}
		seen[b.Key] = true
	}
	want := map[string]bool{}
	for k := range trueOf {
		want[k] = true
	}
	if len(seen) != len(want) {
		t.Errorf("resolved %d borrowers, answer key has %d", len(seen), len(want))
	}

	// Nothing that is not a dollar loan with a principal survives.
	for _, m := range p.Marks {
		if m.Principal <= 0 || m.Price < minPrice || m.Price > maxPrice || math.IsNaN(m.Price) {
			t.Fatalf("bad mark survived: %+v", m)
		}
		if m.Filed <= m.AsOf {
			t.Fatalf("mark filed on or before its own date: %+v", m)
		}
		if !m.Original {
			t.Fatalf("mark not taken from the original filing: %+v", m)
		}
	}
	r := p.Report
	for _, why := range []string{"equity or structured", "no principal", "non-dollar"} {
		if r.Dropped[why] == 0 {
			t.Errorf("expected rows dropped as %q", why)
		}
	}
	if r.RowsSuperseded == 0 {
		t.Error("expected comparative rows to be superseded")
	}
	if r.Families != cfg.Families {
		t.Errorf("found %d manager families, want %d", r.Families, cfg.Families)
	}

	// Funds in one family report identical marks; that is what makes them one
	// observation rather than several.
	type k struct{ inst, asof int }
	byFam := map[k]map[string]float64{}
	for _, m := range p.Marks {
		key := k{m.Instrument, int(m.AsOf)}
		fam := p.Holders[m.Holder].Family
		if byFam[key] == nil {
			byFam[key] = map[string]float64{}
		}
		if prev, ok := byFam[key][fam]; ok && math.Abs(prev-m.Price) > 2e-4 {
			t.Fatalf("family %s reports %.4f and %.4f for one instrument and date", fam, prev, m.Price)
		}
		byFam[key][fam] = m.Price
	}

	for _, s := range []string{"senior", "junior"} {
		found := false
		for _, in := range p.Instruments {
			if in.Class.String() == s {
				found = true
				break
			}
		}
		if !found {
			t.Errorf("no %s instruments", s)
		}
	}
	if strings.TrimSpace(r.FirstAsOf) == "" {
		t.Error("report has no date range")
	}
}
