package fetch

import (
	"bufio"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"time"

	"syndicate/ingest/internal/day"
)

// Series is one FRED series and the column it becomes in the factor table.
type Series struct {
	ID     string
	Column string
}

// Factors are the daily public signals a private mark is rolled forward on.
// The ICE BofA option-adjusted spreads are published on FRED with a rolling
// three-year window, so the Moody's Baa spread carries the longer history.
var Factors = []Series{
	{"BAA10Y", "baa_spread"},
	{"BAMLH0A0HYM2", "hy_oas"},
	{"BAMLH0A2HYB", "b_oas"},
	{"BAMLH0A3HYC", "ccc_oas"},
	{"BAMLC0A4CBBB", "bbb_oas"},
	{"VIXCLS", "vix"},
	{"SP500", "sp500"},
	{"DGS2", "ust2"},
	{"DGS5", "ust5"},
	{"DGS10", "ust10"},
	{"SOFR", "sofr"},
}

const fredCSV = "https://fred.stlouisfed.org/graph/fredgraph.csv?id="

// FRED downloads each series' CSV into dir.
func FRED(dir string, logf func(string, ...any)) error {
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	client := &http.Client{Timeout: 2 * time.Minute}
	for _, s := range Factors {
		resp, err := client.Get(fredCSV + s.ID)
		if err != nil {
			return fmt.Errorf("%s: %w", s.ID, err)
		}
		body, err := io.ReadAll(io.LimitReader(resp.Body, 32<<20))
		resp.Body.Close()
		if err != nil {
			return fmt.Errorf("%s: %w", s.ID, err)
		}
		if resp.StatusCode != http.StatusOK {
			return fmt.Errorf("%s: %s", s.ID, resp.Status)
		}
		if !strings.HasPrefix(string(body), "observation_date,") {
			return fmt.Errorf("%s: response is not a FRED csv", s.ID)
		}
		if err := os.WriteFile(filepath.Join(dir, s.ID+".csv"), body, 0o644); err != nil {
			return err
		}
		logf("fred %s: %d bytes", s.ID, len(body))
		time.Sleep(300 * time.Millisecond)
	}
	return nil
}

func readSeries(path string) (map[day.Date]float64, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	out := map[day.Date]float64{}
	sc := bufio.NewScanner(f)
	sc.Scan() // header
	for sc.Scan() {
		parts := strings.SplitN(sc.Text(), ",", 2)
		if len(parts) != 2 {
			continue
		}
		d := day.Parse(parts[0])
		v, err := strconv.ParseFloat(strings.TrimSpace(parts[1]), 64)
		if d.IsZero() || err != nil {
			continue // FRED writes "." or nothing on days without an observation
		}
		out[d] = v
	}
	return out, sc.Err()
}

// WriteFactors merges the downloaded series into one table with a row per
// day on which any series printed, starting at from. Cells are left empty
// where a series has no observation; nothing is filled or interpolated here,
// because whether a gap is a holiday or missing history is the model's call.
func WriteFactors(rawDir, outPath string, from day.Date) (int, error) {
	cols := make([]map[day.Date]float64, len(Factors))
	days := map[day.Date]struct{}{}
	for i, s := range Factors {
		m, err := readSeries(filepath.Join(rawDir, s.ID+".csv"))
		if err != nil {
			return 0, fmt.Errorf("%s: %w (run fetch-fred first)", s.ID, err)
		}
		cols[i] = m
		for d := range m {
			if d >= from {
				days[d] = struct{}{}
			}
		}
	}
	order := make([]day.Date, 0, len(days))
	for d := range days {
		order = append(order, d)
	}
	sort.Slice(order, func(i, j int) bool { return order[i] < order[j] })

	f, err := os.Create(outPath)
	if err != nil {
		return 0, err
	}
	w := bufio.NewWriter(f)
	w.WriteString("date")
	for _, s := range Factors {
		w.WriteString("\t" + s.Column)
	}
	w.WriteByte('\n')
	for _, d := range order {
		fmt.Fprintf(w, "%d", d)
		for i := range Factors {
			w.WriteByte('\t')
			if v, ok := cols[i][d]; ok {
				w.WriteString(strconv.FormatFloat(v, 'f', -1, 64))
			}
		}
		w.WriteByte('\n')
	}
	if err := w.Flush(); err != nil {
		f.Close()
		return 0, err
	}
	return len(order), f.Close()
}
