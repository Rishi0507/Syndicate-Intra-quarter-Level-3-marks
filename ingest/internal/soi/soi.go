// Package soi streams the Schedule of Investments report out of an SEC BDC
// data set archive.
//
// The SOI report is a pivot the SEC rebuilds for every release: a fixed set of
// metadata, axis and value columns comes first, then any number of extra
// columns whose presence and order change between updates. Columns are
// therefore located by name, never by position, and unknown ones are ignored.
//
// The published files do not quite match the published schema. The columns
// the readme calls "Investment Owned, Fair Value" and "Investment Owned, Cost"
// exist and are empty in every release from 2022 to 2026; the values sit
// under "Initial fair value of Investment" and "Adjusted cost basis", labels
// the pivot evidently took from one filer. Each value is therefore read from
// the first of several candidate columns that has something in it.
package soi

import (
	"archive/zip"
	"bufio"
	"fmt"
	"io"
	"strconv"
	"strings"

	"syndicate/ingest/internal/day"
)

// Row is one holding as a registrant reported it.
type Row struct {
	Adsh   string
	CIK    string
	Holder string
	Form   string
	AsOf   day.Date // ddate: the date the values are measured at
	Filed  day.Date // when the filing reached EDGAR
	Period day.Date // balance sheet date of the filing that carried the row

	Sector      string
	Identifier  string
	Affiliation string
	Type        string
	// Issuer is the borrower as a separate axis, which some filers populate
	// in place of, or alongside, a name inside the identifier.
	Issuer string
	// Entity is set when the holding belongs to a subsidiary or joint
	// venture reported inside the filing, not to the registrant itself.
	Entity   string
	Currency string
	// RateType is the reference rate the loan floats on, as a taxonomy
	// member such as "...#SecuredOvernightFinancingRateSofrMember".
	RateType string
	// Shares is set for equity positions.
	Shares float64

	Rate      float64 // all-in interest rate, as a fraction
	Spread    float64 // margin over the reference rate, as a fraction
	Maturity  day.Date
	Principal float64
	Cost      float64
	FairValue float64

	HasRate, HasSpread, HasPrincipal, HasCost, HasFairValue bool
}

// columns holds the position of each field in one file's header, or -1.
type columns struct {
	adsh, cik, name, form, ddate, filed, period int
	sector, ident, affil, typ                   int
	issuer, entity, currency, shares, rateType  int
	rate, spread, maturity, principal           int
	// Candidate columns for each value, in order of preference.
	cost, fair []int
}

// squash reduces a header to its letters so that "Investment Owned, Fair
// Value" and "investmentownedfairvalue" compare equal.
func squash(s string) string {
	var b strings.Builder
	for _, r := range strings.ToLower(s) {
		if r >= 'a' && r <= 'z' {
			b.WriteRune(r)
		}
	}
	return b.String()
}

func locate(header []string) (columns, error) {
	var c columns
	idx := map[string]int{}
	for i, h := range header {
		k := squash(h)
		if _, dup := idx[k]; !dup {
			idx[k] = i
		}
	}
	find := func(names ...string) int {
		for _, n := range names {
			if i, ok := idx[n]; ok {
				return i
			}
		}
		return -1
	}
	c.adsh = find("adsh")
	c.cik = find("cik")
	c.name = find("name")
	c.form = find("form")
	c.ddate = find("ddate")
	c.filed = find("filed")
	c.period = find("period")
	c.sector = find("industrysectoraxis")
	c.ident = find("investmentidentifieraxis")
	c.affil = find("investmentissueraffiliationaxis")
	c.typ = find("investmenttypeaxis")
	c.rate = find("investmentinterestrate")
	c.spread = find("investmentbasisspreadvariablerate")
	c.maturity = find("investmentmaturitydate")
	c.principal = find("investmentownedbalanceprincipalamount")
	c.issuer = find("investmentissuernameaxis")
	c.entity = find("legalentityaxis")
	c.currency = find("currencyaxis")
	c.rateType = find("investmentvariableinterestratetypeextensibleenumeration")
	c.shares = find("investmentshares", "investmentownedbalanceshares")
	all := func(names ...string) []int {
		var out []int
		for _, n := range names {
			if i, ok := idx[n]; ok {
				out = append(out, i)
			}
		}
		return out
	}
	c.cost = all("investmentownedcost", "investmentownedatcost", "adjustedcostbasis")
	c.fair = all("investmentownedfairvalue", "investmentownedatfairvalue", "initialfairvalueofinvestment")

	var missing []string
	for _, r := range []struct {
		name string
		pos  int
	}{{"cik", c.cik}, {"name", c.name}, {"ddate", c.ddate}, {"filed", c.filed},
		{"Investment, Identifier Axis", c.ident}} {
		if r.pos < 0 {
			missing = append(missing, r.name)
		}
	}
	if len(c.fair) == 0 {
		missing = append(missing, "a fair value column")
	}
	if len(missing) > 0 {
		return c, fmt.Errorf("soi header is missing required columns: %s", strings.Join(missing, "; "))
	}
	return c, nil
}

func field(parts []string, i int) string {
	if i < 0 || i >= len(parts) {
		return ""
	}
	return strings.TrimSpace(parts[i])
}

// firstNumber reads a value from the first candidate column that holds one.
func firstNumber(parts []string, candidates []int) (float64, bool) {
	for _, i := range candidates {
		if v, ok := number(parts, i); ok {
			return v, true
		}
	}
	return 0, false
}

func number(parts []string, i int) (float64, bool) {
	s := field(parts, i)
	if s == "" {
		return 0, false
	}
	v, err := strconv.ParseFloat(strings.ReplaceAll(s, ",", ""), 64)
	if err != nil {
		return 0, false
	}
	return v, true
}

// Stats counts what a read pass saw.
type Stats struct {
	Lines   int
	Rows    int
	Skipped int // lines too short to hold the required columns
}

// Read parses a soi.tsv stream and calls fn for every row. Returning an error
// from fn stops the read.
func Read(r io.Reader, fn func(*Row) error) (Stats, error) {
	var st Stats
	br := bufio.NewReaderSize(r, 1<<20)

	line, err := readLine(br)
	if err == io.EOF && strings.TrimSpace(line) == "" {
		// A month in which no BDC filed a schedule is published as an empty
		// report. That is a month with nothing in it, not a broken archive.
		return st, nil
	}
	if err != nil && err != io.EOF {
		return st, fmt.Errorf("reading soi header: %w", err)
	}
	// A UTF-8 byte order mark would otherwise glue itself to the first name.
	line = strings.TrimPrefix(line, "\xef\xbb\xbf")
	cols, err := locate(strings.Split(line, "\t"))
	if err != nil {
		return st, err
	}

	var row Row
	for {
		line, err = readLine(br)
		if err == io.EOF && line == "" {
			break
		}
		if err != nil && err != io.EOF {
			return st, err
		}
		st.Lines++
		parts := strings.Split(line, "\t")
		if len(parts) <= cols.ident || len(parts) <= cols.ddate {
			st.Skipped++
		} else {
			row = Row{
				Adsh:        field(parts, cols.adsh),
				CIK:         strings.TrimLeft(field(parts, cols.cik), "0"),
				Holder:      field(parts, cols.name),
				Form:        field(parts, cols.form),
				AsOf:        day.Parse(field(parts, cols.ddate)),
				Filed:       day.Parse(field(parts, cols.filed)),
				Period:      day.Parse(field(parts, cols.period)),
				Sector:      field(parts, cols.sector),
				Identifier:  field(parts, cols.ident),
				Affiliation: field(parts, cols.affil),
				Type:        field(parts, cols.typ),
				Issuer:      field(parts, cols.issuer),
				Entity:      field(parts, cols.entity),
				Currency:    field(parts, cols.currency),
				RateType:    field(parts, cols.rateType),
				Maturity:    day.Parse(field(parts, cols.maturity)),
			}
			row.Shares, _ = number(parts, cols.shares)
			row.Rate, row.HasRate = number(parts, cols.rate)
			row.Spread, row.HasSpread = number(parts, cols.spread)
			row.Principal, row.HasPrincipal = number(parts, cols.principal)
			row.Cost, row.HasCost = firstNumber(parts, cols.cost)
			row.FairValue, row.HasFairValue = firstNumber(parts, cols.fair)
			st.Rows++
			if e := fn(&row); e != nil {
				return st, e
			}
		}
		if err == io.EOF {
			break
		}
	}
	return st, nil
}

// readLine returns one line without its terminator, however long it is.
func readLine(br *bufio.Reader) (string, error) {
	var sb strings.Builder
	for {
		chunk, isPrefix, err := br.ReadLine()
		if err != nil {
			return sb.String(), err
		}
		sb.Write(chunk)
		if !isPrefix {
			return sb.String(), nil
		}
	}
}

// ReadZip finds the SOI report inside a BDC data set archive and streams it.
// The archive is never unpacked to disk.
func ReadZip(path string, fn func(*Row) error) (Stats, error) {
	zr, err := zip.OpenReader(path)
	if err != nil {
		return Stats{}, err
	}
	defer zr.Close()

	for _, f := range zr.File {
		base := strings.ToLower(f.Name)
		if i := strings.LastIndexAny(base, "/\\"); i >= 0 {
			base = base[i+1:]
		}
		if base != "soi.tsv" {
			continue
		}
		rc, err := f.Open()
		if err != nil {
			return Stats{}, err
		}
		st, err := Read(rc, fn)
		rc.Close()
		return st, err
	}
	return Stats{}, fmt.Errorf("%s: no soi.tsv in archive", path)
}
