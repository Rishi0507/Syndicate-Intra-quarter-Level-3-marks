package resolve

import "testing"

func TestKeys(t *testing.T) {
	cases := []struct{ name, strict, core string }{
		{"Acme Software Buyer, Inc.", "acme software buyer", "acme software"},
		{"ACME SOFTWARE INTERMEDIATE HOLDINGS LLC", "acme software intermediate holding", "acme software"},
		{"Acme Technologies, L.P.", "acme technology", "acme technology"},
		{"Smith & Jones Svcs. Corp", "smith jone service", "smith jone service"},
		{"Société Générale S.à r.l.", "societe generale", "societe generale"},
		// Nothing but structure: fall back so the key is not empty.
		{"Intermediate Holdings LLC", "intermediate holding", "intermediate holding"},
		{"Harbor Logistics Parent II, LLC", "harbor logistic parent ii", "harbor logistic"},
	}
	for _, c := range cases {
		if got := Strict(c.name); got != c.strict {
			t.Errorf("Strict(%q) = %q, want %q", c.name, got, c.strict)
		}
		if got := Core(c.name); got != c.core {
			t.Errorf("Core(%q) = %q, want %q", c.name, got, c.core)
		}
	}
}

func same(t *testing.T, assign map[string]int, a, b string, want bool) {
	t.Helper()
	if got := assign[a] == assign[b]; got != want {
		t.Errorf("same(%q, %q) = %v, want %v", a, b, got, want)
	}
}

func TestResolve(t *testing.T) {
	r := New()
	loan := MakeTermKey(202906, 575)
	other := MakeTermKey(203011, 500)

	// One company under three vehicle names: merged on the core name alone.
	r.Add("Acme Software Buyer, Inc.", nil, 10, loan, "A")
	r.Add("Acme Software Holdings LLC", nil, 30, loan, "B")
	r.Add("Acme Software Parent, L.P.", nil, 5, 0, "C")

	// Two different companies sharing one distinctive word and no terms.
	r.Add("Summit Holdings, LLC", nil, 10, loan, "A")
	r.Add("Summit Acquisition Corp", nil, 10, other, "B")

	// The same single-word company, corroborated by identical loan terms.
	r.Add("Zenith Buyer LLC", nil, 10, other, "A")
	r.Add("Zenith Intermediate, Inc.", nil, 10, other, "B")

	// A typo, corroborated.
	r.Add("Meridian Dental Managment LLC", nil, 10, loan, "A")
	r.Add("Meridian Dental Management, LLC", nil, 10, loan, "B")

	// Similar names, different loans: left apart.
	r.Add("Harbor Energy Services", nil, 10, loan, "A")
	r.Add("Harbor Engineering Services", nil, 10, other, "B")

	// A disclosed alias.
	r.Add("Lumenworks Technology Group", nil, 10, loan, "A")
	r.Add("Keystone Brands LLC", []string{"Lumenworks Technology Group"}, 10, other, "B")

	assign, clusters, _ := r.Resolve()

	same(t, assign, "Acme Software Buyer, Inc.", "Acme Software Holdings LLC", true)
	same(t, assign, "Acme Software Buyer, Inc.", "Acme Software Parent, L.P.", true)
	same(t, assign, "Summit Holdings, LLC", "Summit Acquisition Corp", false)
	same(t, assign, "Zenith Buyer LLC", "Zenith Intermediate, Inc.", true)
	same(t, assign, "Meridian Dental Managment LLC", "Meridian Dental Management, LLC", true)
	same(t, assign, "Harbor Energy Services", "Harbor Engineering Services", false)
	same(t, assign, "Lumenworks Technology Group", "Keystone Brands LLC", true)

	// A cluster is shown under the spelling with the most value behind it.
	if c := clusters[assign["Acme Software Buyer, Inc."]]; c.Name != "Acme Software Holdings LLC" || c.Holders != 3 {
		t.Errorf("acme cluster = %+v", c)
	}
}

func TestEditDistance(t *testing.T) {
	for _, c := range []struct {
		a, b string
		max  int
		want int
	}{{"kitten", "sitting", 3, 3}, {"abc", "abc", 1, 0}, {"abc", "abd", 1, 1}, {"abcdef", "uvwxyz", 2, 3}} {
		if got := editDistance(c.a, c.b, c.max); got != c.want {
			t.Errorf("editDistance(%q, %q, %d) = %d, want %d", c.a, c.b, c.max, got, c.want)
		}
	}
}
