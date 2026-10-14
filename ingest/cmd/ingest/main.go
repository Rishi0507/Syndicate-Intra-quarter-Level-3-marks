package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"syndicate/ingest/internal/day"
	"syndicate/ingest/internal/fetch"
	"syndicate/ingest/internal/ident"
	"syndicate/ingest/internal/panel"
	"syndicate/ingest/internal/soi"
	"syndicate/ingest/internal/synth"
)

func logf(format string, args ...any) {
	fmt.Fprintf(os.Stderr, format+"\n", args...)
}

func fail(err error) {
	fmt.Fprintln(os.Stderr, "ingest:", err)
	os.Exit(1)
}

type source struct {
	Kind     string `json:"kind"`
	Built    string `json:"built"`
	Note     string `json:"note"`
	Archives int    `json:"archives"`
}

func writeSource(dir string, s source) error {
	s.Built = time.Now().UTC().Format(time.RFC3339)
	js, err := json.MarshalIndent(s, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(dir, "source.json"), append(js, '\n'), 0o644)
}

func archivesIn(dir string) ([]string, error) {
	got, err := filepath.Glob(filepath.Join(dir, "*_bdc.zip"))
	if err != nil {
		return nil, err
	}
	sort.Strings(got)
	return got, nil
}

func copyFile(src, dst string) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()
	out, err := os.Create(dst)
	if err != nil {
		return err
	}
	if _, err := io.Copy(out, in); err != nil {
		out.Close()
		return err
	}
	return out.Close()
}

func summarise(p *panel.Panel) {
	r := p.Report
	logf("")
	logf("  rows read          %10d", r.RowsRead)
	logf("  superseded         %10d   (comparatives and amendments)", r.RowsSuperseded)
	if r.RowsDuplicate > 0 {
		logf("  already read       %10d   (filings present in more than one archive)", r.RowsDuplicate)
	}
	if r.RowsAfterCutoff > 0 {
		logf("  filed after %s %8d   (ignored)", r.Until, r.RowsAfterCutoff)
	}
	logf("  positions kept     %10d", r.RowsKept)
	reasons := make([]string, 0, len(r.Dropped))
	for k := range r.Dropped {
		reasons = append(reasons, k)
	}
	sort.Slice(reasons, func(i, j int) bool { return r.Dropped[reasons[i]] > r.Dropped[reasons[j]] })
	for _, k := range reasons {
		logf("    dropped %-22s %8d", k, r.Dropped[k])
	}
	logf("  holders            %10d   in %d manager families", r.Holders, r.Families)
	logf("  borrowers          %10d   from %d spellings", r.Borrowers, r.Resolve.Mentions)
	logf("  instruments        %10d   %d seen by 2+ holders, %d by 2+ families", r.Instruments, r.MultiHolder, r.MultiFamily)
	logf("  marks              %10d   %s to %s", r.Marks, r.FirstAsOf, r.LastAsOf)
	logf("  tranche marks      %10d", r.TrancheMarks)
	logf("  restated later     %10d   of %d rows compared moved over 0.1 points, %d over 1 point",
		r.Restated.Changed, r.Restated.Compared, r.Restated.ChangedOne)
}

func main() {
	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, "usage: ingest <fetch-sec|fetch-edgar|fetch-fred|build|synth|inspect> [flags]")
		os.Exit(2)
	}
	cmd, args := os.Args[1], os.Args[2:]
	fs := flag.NewFlagSet(cmd, flag.ExitOnError)

	switch cmd {
	case "fetch-sec":
		out := fs.String("out", "data/raw/sec", "directory for the downloaded archives")
		fs.Parse(args)
		got, err := fetch.SEC(*out, os.Getenv("SEC_CONTACT"), logf)
		if err != nil {
			fail(err)
		}
		logf("%d archives in %s", len(got), *out)

	case "fetch-edgar":
		dir := fs.String("sec", "data/raw/sec", "directory holding the SEC *_bdc.zip archives")
		out := fs.String("out", "", "directory for the filled archives (default: the -sec directory)")
		cache := fs.String("cache", "data/raw/edgar", "directory for downloaded filing documents")
		month := fs.String("month", "", "comma-separated yyyy-mm months to read even if the data sets cover them")
		fs.Parse(args)
		var months []string
		for _, m := range strings.Split(*month, ",") {
			if m = strings.TrimSpace(m); m != "" {
				months = append(months, m)
			}
		}
		got, err := fetch.EDGAR(fetch.EdgarOptions{Dir: *dir, Out: *out, Cache: *cache, Contact: os.Getenv("SEC_CONTACT"), Months: months}, logf)
		if err != nil {
			fail(err)
		}
		logf("%d filled archives", len(got))

	case "fetch-fred":
		out := fs.String("out", "data/raw/fred", "directory for the downloaded series")
		fs.Parse(args)
		if err := fetch.FRED(*out, logf); err != nil {
			fail(err)
		}

	case "build":
		sec := fs.String("sec", "data/raw/sec", "directory holding *_bdc.zip archives")
		fred := fs.String("fred", "data/raw/fred", "directory holding FRED csv files")
		out := fs.String("out", "data/build", "output directory")
		from := fs.Int("from", 20220601, "first date of the factor table, yyyymmdd")
		until := fs.String("until", "", "ignore rows filed after this date, yyyy-mm-dd, to rebuild the panel as it stood then")
		fs.Parse(args)

		archives, err := archivesIn(*sec)
		if err != nil {
			fail(err)
		}
		if len(archives) == 0 {
			fail(fmt.Errorf("no *_bdc.zip archives in %s; run fetch-sec, or download them from the SEC's BDC data sets page", *sec))
		}
		p, err := panel.BuildUntil(archives, day.Parse(*until), logf)
		if err != nil {
			fail(err)
		}
		if err := p.Write(*out); err != nil {
			fail(err)
		}
		n, err := fetch.WriteFactors(*fred, filepath.Join(*out, "factors.tsv"), day.Date(*from))
		if err != nil {
			fail(err)
		}
		if err := writeSource(*out, source{Kind: "sec", Archives: len(archives),
			Note: "SEC BDC data sets, as filed. Public market series from FRED."}); err != nil {
			fail(err)
		}
		summarise(p)
		logf("  factor days        %10d", n)
		logf("wrote %s", *out)

	case "synth":
		out := fs.String("out", "data/synth", "output directory")
		seed := fs.Int64("seed", synth.Default().Seed, "random seed")
		fs.Parse(args)

		cfg := synth.Default()
		cfg.Seed = *seed
		if _, err := synth.Generate(cfg, *out); err != nil {
			fail(err)
		}
		archives, err := archivesIn(filepath.Join(*out, "sec"))
		if err != nil {
			fail(err)
		}
		p, err := panel.Build(archives, func(string, ...any) {})
		if err != nil {
			fail(err)
		}
		build := filepath.Join(*out, "build")
		if err := p.Write(build); err != nil {
			fail(err)
		}
		for _, name := range []string{"factors.tsv", "truth_prices.tsv", "truth_families.tsv"} {
			if err := copyFile(filepath.Join(*out, name), filepath.Join(build, name)); err != nil {
				fail(err)
			}
		}
		if err := writeSource(build, source{Kind: "synthetic", Archives: len(archives),
			Note: "Generated universe with a known answer. No real company, lender or mark."}); err != nil {
			fail(err)
		}
		summarise(p)
		logf("wrote %s", build)

	case "inspect":
		zipPath := fs.String("zip", "", "archive to read")
		per := fs.Int("n", 4, "samples per registrant")
		top := fs.Int("top", 40, "registrants to show, largest first")
		only := fs.String("only", "", "show only registrants whose name contains this")
		fs.Parse(args)
		type reg struct {
			name    string
			rows    int
			liens   map[string]int
			samples []string
		}
		regs := map[string]*reg{}
		_, err := soi.ReadZip(*zipPath, func(r *soi.Row) error {
			if r.Identifier == "" || !r.HasFairValue || r.Period != r.AsOf {
				return nil
			}
			g := regs[r.CIK]
			if g == nil {
				g = &reg{name: r.Holder, liens: map[string]int{}}
				regs[r.CIK] = g
			}
			g.rows++
			p := ident.Parse(r.Identifier, r.Type)
			g.liens[p.Lien.String()]++
			if g.rows%97 == 1 && len(g.samples) < *per {
				g.samples = append(g.samples, fmt.Sprintf("    %q\n      type %q issuer %q\n      -> %q | %s | %s",
					r.Identifier, r.Type, r.Issuer, p.Borrower, p.Lien, p.Facility))
			}
			return nil
		})
		if err != nil {
			fail(err)
		}
		list := make([]*reg, 0, len(regs))
		for _, g := range regs {
			if *only == "" || strings.Contains(strings.ToUpper(g.name), strings.ToUpper(*only)) {
				list = append(list, g)
			}
		}
		sort.Slice(list, func(i, j int) bool { return list[i].rows > list[j].rows })
		if len(list) > *top {
			list = list[:*top]
		}
		for _, g := range list {
			fmt.Printf("\n%s  (%d rows)  %v\n", g.name, g.rows, g.liens)
			for _, s := range g.samples {
				fmt.Println(s)
			}
		}

	default:
		fmt.Fprintf(os.Stderr, "ingest: unknown command %q\n", cmd)
		os.Exit(2)
	}
}
