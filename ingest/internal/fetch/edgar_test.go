package fetch

import (
	"reflect"
	"testing"
	"time"
)

func TestMonthsOfArchiveNames(t *testing.T) {
	for name, want := range map[string][]string{
		"2025q4_bdc.zip":        {"2025-10", "2025-11", "2025-12"},
		"2026_07_bdc.zip":       {"2026-07"},
		"edgar_2026_08_bdc.zip": nil,
		"readme.txt":            nil,
	} {
		if got := monthsOf(name); !reflect.DeepEqual(got, want) {
			t.Errorf("monthsOf(%q) = %v, want %v", name, got, want)
		}
	}
}

func TestMissingMonths(t *testing.T) {
	c := covered{months: map[string]bool{}}
	for _, n := range []string{"2026q1_bdc.zip", "2026_04_bdc.zip", "2026_05_bdc.zip", "2026_07_bdc.zip", "2026_09_bdc.zip"} {
		for _, m := range monthsOf(n) {
			c.months[m] = true
		}
	}
	got := missingMonths(c, time.Date(2026, 10, 14, 0, 0, 0, 0, time.UTC))
	want := []string{"2026-06", "2026-08", "2026-10"}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("missingMonths = %v, want %v", got, want)
	}
}
