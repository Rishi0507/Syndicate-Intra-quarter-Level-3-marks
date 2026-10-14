package panel

import (
	"bufio"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

func clean(s string) string {
	return strings.Join(strings.Fields(strings.NewReplacer("\t", " ", "\n", " ", "\r", " ").Replace(s)), " ")
}

func writeTSV(path string, header string, n int, row func(i int, w *bufio.Writer)) error {
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	w := bufio.NewWriterSize(f, 1<<20)
	fmt.Fprintln(w, header)
	for i := 0; i < n; i++ {
		row(i, w)
	}
	if err := w.Flush(); err != nil {
		f.Close()
		return err
	}
	return f.Close()
}

func (p *Panel) Write(dir string) error {
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	if err := writeTSV(filepath.Join(dir, "holders.tsv"),
		"holder\tcik\tname\tfamily\tfirst\tlast\tmarks", len(p.Holders),
		func(i int, w *bufio.Writer) {
			h := p.Holders[i]
			fmt.Fprintf(w, "%d\t%s\t%s\t%s\t%d\t%d\t%d\n", h.ID, h.CIK, clean(h.Name), clean(h.Family), h.First, h.Last, h.Marks)
		}); err != nil {
		return err
	}
	if err := writeTSV(filepath.Join(dir, "borrowers.tsv"),
		"borrower\tname\tkey\tsector\tspellings\tholders\tfamilies", len(p.Borrowers),
		func(i int, w *bufio.Writer) {
			b := p.Borrowers[i]
			fmt.Fprintf(w, "%d\t%s\t%s\t%s\t%d\t%d\t%d\n", b.ID, clean(b.Name), clean(b.Key), b.Sector, b.Spellings, b.Holders, b.Families)
		}); err != nil {
		return err
	}
	if err := writeTSV(filepath.Join(dir, "instruments.tsv"),
		"instrument\tborrower\tclass\tholders\tfamilies\tmarks\tfirst\tlast\tpiece", len(p.Instruments),
		func(i int, w *bufio.Writer) {
			n := p.Instruments[i]
			fmt.Fprintf(w, "%d\t%d\t%s\t%d\t%d\t%d\t%d\t%d\t%d\n", n.ID, n.Borrower, n.Class, n.Holders, n.Families, n.Marks, n.First, n.Last, n.Piece)
		}); err != nil {
		return err
	}
	if err := writeTSV(filepath.Join(dir, "marks.tsv"),
		"instrument\tholder\tasof\tfiled\toriginal\tprice\tprincipal\tfair_value\tcost\tspread_bp\tmaturity\trows", len(p.Marks),
		func(i int, w *bufio.Writer) {
			m := p.Marks[i]
			orig := 0
			if m.Original {
				orig = 1
			}
			fmt.Fprintf(w, "%d\t%d\t%d\t%d\t%d\t%.6f\t%.2f\t%.2f\t%.2f\t%.1f\t%d\t%d\n",
				m.Instrument, m.Holder, m.AsOf, m.Filed, orig, m.Price, m.Principal, m.FairValue, m.Cost, m.SpreadBp, m.Maturity, m.Rows)
		}); err != nil {
		return err
	}
	if err := writeTSV(filepath.Join(dir, "tranches.tsv"),
		"borrower\tclass\tmaturity_ym\tspread_bp\tholder\tasof\tprice\tprincipal", len(p.Tranches),
		func(i int, w *bufio.Writer) {
			t := p.Tranches[i]
			fmt.Fprintf(w, "%d\t%s\t%d\t%d\t%d\t%d\t%.6f\t%.2f\n", t.Borrower, t.Class, t.MaturityYM, t.SpreadBp, t.Holder, t.AsOf, t.Price, t.Principal)
		}); err != nil {
		return err
	}
	r := p.Report
	audit := [][2]string{
		{"rows_read", fmt.Sprint(r.RowsRead)},
		{"rows_superseded", fmt.Sprint(r.RowsSuperseded)},
		{"rows_kept", fmt.Sprint(r.RowsKept)},
		{"marks", fmt.Sprint(r.Marks)},
		{"restated_compared", fmt.Sprint(r.Restated.Compared)},
		{"restated_over_tenth_point", fmt.Sprint(r.Restated.Changed)},
		{"restated_over_one_point", fmt.Sprint(r.Restated.ChangedOne)},
		{"restated_mean_abs_points", fmt.Sprint(r.Restated.MeanAbsDiff)},
		{"filed_until", r.Until},
	}
	if err := writeTSV(filepath.Join(dir, "audit.tsv"), "measure\tvalue", len(audit),
		func(i int, w *bufio.Writer) {
			fmt.Fprintf(w, "%s\t%s\n", audit[i][0], audit[i][1])
		}); err != nil {
		return err
	}
	js, err := json.MarshalIndent(r, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(dir, "ingest_report.json"), append(js, '\n'), 0o644)
}
