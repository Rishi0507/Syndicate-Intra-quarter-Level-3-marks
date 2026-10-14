package fetch

import (
	"archive/zip"
	"bufio"
	"bytes"
	"compress/gzip"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"time"

	"syndicate/ingest/internal/xbrl"
)

const (
	edgarSubmissions = "https://data.sec.gov/submissions/CIK%010s.json"
	edgarArchive     = "https://www.sec.gov/Archives/edgar/data/%s/%s/"
	edgarPause       = 150 * time.Millisecond
)

var (
	secArchiveRe   = regexp.MustCompile(`^([0-9]{4})(?:q([1-4])|_([0-9]{2}))_bdc\.zip$`)
	edgarArchiveRe = regexp.MustCompile(`^edgar_([0-9]{4})_([0-9]{2})_bdc\.zip$`)
	periodicForms  = map[string]bool{"10-Q": true, "10-K": true, "10-Q/A": true, "10-K/A": true, "10-KT": true}
)

type EdgarOptions struct {
	Dir     string
	Out     string
	Cache   string
	Contact string
	Months  []string
	Now     time.Time
}

type covered struct {
	months map[string]bool
	adsh   map[string]bool
	ciks   map[string]string
}

func monthsOf(name string) []string {
	m := secArchiveRe.FindStringSubmatch(name)
	if m == nil {
		return nil
	}
	if m[2] != "" {
		q := int(m[2][0] - '0')
		var out []string
		for k := 0; k < 3; k++ {
			out = append(out, fmt.Sprintf("%s-%02d", m[1], 3*(q-1)+k+1))
		}
		return out
	}
	return []string{m[1] + "-" + m[3]}
}

func scanArchives(dir string) (covered, error) {
	c := covered{months: map[string]bool{}, adsh: map[string]bool{}, ciks: map[string]string{}}
	entries, err := os.ReadDir(dir)
	if err != nil {
		return c, err
	}
	for _, e := range entries {
		months := monthsOf(e.Name())
		if months == nil {
			continue
		}
		for _, m := range months {
			c.months[m] = true
		}
		if err := readSub(filepath.Join(dir, e.Name()), &c); err != nil {
			return c, fmt.Errorf("%s: %w", e.Name(), err)
		}
	}
	return c, nil
}

func readSub(path string, c *covered) error {
	zr, err := zip.OpenReader(path)
	if err != nil {
		return err
	}
	defer zr.Close()
	for _, f := range zr.File {
		if !strings.HasSuffix(strings.ToLower(f.Name), "sub.tsv") {
			continue
		}
		rc, err := f.Open()
		if err != nil {
			return err
		}
		defer rc.Close()
		sc := bufio.NewScanner(rc)
		sc.Buffer(make([]byte, 1<<20), 1<<24)
		col := map[string]int{}
		for first := true; sc.Scan(); first = false {
			parts := strings.Split(sc.Text(), "\t")
			if first {
				for i, h := range parts {
					col[strings.ToLower(strings.TrimSpace(h))] = i
				}
				continue
			}
			get := func(k string) string {
				if i, ok := col[k]; ok && i < len(parts) {
					return strings.TrimSpace(parts[i])
				}
				return ""
			}
			if a := get("adsh"); a != "" {
				c.adsh[a] = true
			}
			if k := strings.TrimLeft(get("cik"), "0"); k != "" {
				c.ciks[k] = get("name")
			}
		}
		return sc.Err()
	}
	return fmt.Errorf("no sub.tsv in archive")
}

func missingMonths(c covered, now time.Time) []string {
	first := ""
	for m := range c.months {
		if first == "" || m < first {
			first = m
		}
	}
	if first == "" {
		return nil
	}
	var out []string
	t, _ := time.Parse("2006-01", first)
	end := time.Date(now.Year(), now.Month(), 1, 0, 0, 0, 0, time.UTC)
	for ; !t.After(end); t = t.AddDate(0, 1, 0) {
		if m := t.Format("2006-01"); !c.months[m] {
			out = append(out, m)
		}
	}
	return out
}

type submissions struct {
	Name    string `json:"name"`
	Filings struct {
		Recent struct {
			Accession  []string `json:"accessionNumber"`
			FilingDate []string `json:"filingDate"`
			ReportDate []string `json:"reportDate"`
			Form       []string `json:"form"`
		} `json:"recent"`
	} `json:"filings"`
}

type index struct {
	Directory struct {
		Item []struct {
			Name string `json:"name"`
		} `json:"item"`
	} `json:"directory"`
}

func (c *secClient) fetchText(url string) ([]byte, error) {
	if wait := edgarPause - time.Since(c.last); wait > 0 {
		time.Sleep(wait)
	}
	c.last = time.Now()
	req, err := http.NewRequest(http.MethodGet, url, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("User-Agent", c.ua)
	resp, err := c.http.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode == http.StatusForbidden {
		return nil, fmt.Errorf("sec.gov refused the request (403). It rejects clients whose User-Agent does not carry a reachable contact; check SEC_CONTACT")
	}
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("GET %s: %s", url, resp.Status)
	}
	return io.ReadAll(resp.Body)
}

func (c *secClient) cached(cache, name, url string) ([]byte, error) {
	path := filepath.Join(cache, name+".gz")
	if f, err := os.Open(path); err == nil {
		defer f.Close()
		if zr, err := gzip.NewReader(f); err == nil {
			if b, err := io.ReadAll(zr); err == nil && len(b) > 0 {
				return b, nil
			}
		}
	}
	b, err := c.fetchText(url)
	if err != nil {
		return nil, err
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return nil, err
	}
	var buf bytes.Buffer
	zw := gzip.NewWriter(&buf)
	if _, err := zw.Write(b); err != nil {
		return nil, err
	}
	if err := zw.Close(); err != nil {
		return nil, err
	}
	return b, os.WriteFile(path, buf.Bytes(), 0o644)
}

type edgarFiling struct {
	xbrl.Filing
	month string
}

func EDGAR(o EdgarOptions, logf func(string, ...any)) ([]string, error) {
	contact := strings.TrimSpace(o.Contact)
	if contact == "" || !strings.Contains(contact, "@") {
		return nil, ErrNoContact
	}
	if o.Now.IsZero() {
		o.Now = time.Now().UTC()
	}
	if o.Out == "" {
		o.Out = o.Dir
	}
	cov, err := scanArchives(o.Dir)
	if err != nil {
		return nil, err
	}
	if len(cov.months) == 0 {
		return nil, fmt.Errorf("no SEC data set archives in %s; run fetch-sec first", o.Dir)
	}
	months := o.Months
	if len(months) == 0 {
		months = missingMonths(cov, o.Now)
		if err := pruneCovered(o.Out, cov, logf); err != nil {
			return nil, err
		}
	}
	if len(months) == 0 {
		logf("the SEC data sets cover every month; nothing to fill")
		return nil, nil
	}
	want := map[string]bool{}
	for _, m := range months {
		want[m] = true
	}
	logf("filling %s from EDGAR for %d registrants", strings.Join(months, ", "), len(cov.ciks))

	c := &secClient{http: &http.Client{Timeout: 5 * time.Minute}, ua: "Syndicate-Research " + contact}
	ciks := make([]string, 0, len(cov.ciks))
	for k := range cov.ciks {
		ciks = append(ciks, k)
	}
	sort.Strings(ciks)

	var picked []edgarFiling
	for _, cik := range ciks {
		body, err := c.fetchText(fmt.Sprintf(edgarSubmissions, cik))
		if err != nil {
			logf("  %s: %v", cik, err)
			continue
		}
		var s submissions
		if err := json.Unmarshal(body, &s); err != nil {
			return nil, fmt.Errorf("submissions for %s: %w", cik, err)
		}
		r := s.Filings.Recent
		name := cov.ciks[cik]
		if name == "" {
			name = strings.ToUpper(s.Name)
		}
		for i := range r.Accession {
			if i >= len(r.FilingDate) || i >= len(r.Form) || i >= len(r.ReportDate) {
				break
			}
			if !periodicForms[r.Form[i]] || len(r.FilingDate[i]) < 7 || !want[r.FilingDate[i][:7]] {
				continue
			}
			if len(o.Months) == 0 && cov.adsh[r.Accession[i]] {
				continue
			}
			picked = append(picked, edgarFiling{
				Filing: xbrl.Filing{Adsh: r.Accession[i], CIK: cik, Name: name, Form: r.Form[i], Filed: r.FilingDate[i], Period: r.ReportDate[i]},
				month:  r.FilingDate[i][:7],
			})
		}
	}
	logf("%d periodic filings to read", len(picked))

	byMonth := map[string][]xbrl.Row{}
	manifest := map[string][]string{}
	for _, f := range picked {
		rows, err := c.convert(o.Cache, f.Filing)
		if err != nil {
			logf("  %s %s: %v", f.Name, f.Adsh, err)
			continue
		}
		logf("  %-48s %-7s %s  %5d rows", trim(f.Name, 48), f.Form, f.Filed, len(rows))
		byMonth[f.month] = append(byMonth[f.month], rows...)
		manifest[f.month] = append(manifest[f.month], strings.Join([]string{f.Adsh, f.CIK, f.Name, f.Form, f.Filed, f.Period, fmt.Sprint(len(rows))}, "\t"))
	}

	var written []string
	for _, m := range months {
		if len(manifest[m]) == 0 {
			continue
		}
		if err := os.MkdirAll(o.Out, 0o755); err != nil {
			return written, err
		}
		dst := filepath.Join(o.Out, "edgar_"+strings.Replace(m, "-", "_", 1)+"_bdc.zip")
		if err := writeArchive(dst, byMonth[m], manifest[m]); err != nil {
			return written, err
		}
		logf("wrote %s: %d filings, %d rows", dst, len(manifest[m]), len(byMonth[m]))
		written = append(written, dst)
	}
	return written, nil
}

func trim(s string, n int) string {
	if len(s) <= n {
		return s
	}
	return s[:n]
}

func pruneCovered(dir string, cov covered, logf func(string, ...any)) error {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return err
	}
	for _, e := range entries {
		m := edgarArchiveRe.FindStringSubmatch(e.Name())
		if m == nil || !cov.months[m[1]+"-"+m[2]] {
			continue
		}
		logf("removing %s: the SEC data set now covers that month", e.Name())
		if err := os.Remove(filepath.Join(dir, e.Name())); err != nil {
			return err
		}
	}
	return nil
}

func (c *secClient) convert(cache string, f xbrl.Filing) ([]xbrl.Row, error) {
	folder := strings.ReplaceAll(f.Adsh, "-", "")
	base := fmt.Sprintf(edgarArchive, f.CIK, folder)
	dir := filepath.Join(cache, f.CIK, folder)

	body, err := c.cached(dir, "index.json", base+"index.json")
	if err != nil {
		return nil, err
	}
	var idx index
	if err := json.Unmarshal(body, &idx); err != nil {
		return nil, fmt.Errorf("index: %w", err)
	}
	var instance, labels string
	for _, it := range idx.Directory.Item {
		n := strings.ToLower(it.Name)
		switch {
		case strings.HasSuffix(n, "_htm.xml"):
			instance = it.Name
		case strings.HasSuffix(n, "_lab.xml"):
			labels = it.Name
		}
	}
	if instance == "" {
		return nil, fmt.Errorf("no XBRL instance in the filing")
	}
	inst, err := c.cached(dir, instance, base+instance)
	if err != nil {
		return nil, err
	}
	in, err := xbrl.ParseInstance(bytes.NewReader(inst))
	if err != nil {
		return nil, err
	}
	names := map[string]string{}
	if labels != "" {
		lab, err := c.cached(dir, labels, base+labels)
		if err != nil {
			return nil, err
		}
		if names, err = xbrl.ParseLabels(bytes.NewReader(lab)); err != nil {
			return nil, err
		}
	}
	return in.Rows(f, names), nil
}

func writeArchive(dst string, rows []xbrl.Row, manifest []string) error {
	tmp := dst + ".part"
	f, err := os.Create(tmp)
	if err != nil {
		return err
	}
	zw := zip.NewWriter(f)
	write := func(name, header string, lines []string) error {
		w, err := zw.Create(name)
		if err != nil {
			return err
		}
		if _, err := io.WriteString(w, header+"\n"); err != nil {
			return err
		}
		for _, l := range lines {
			if _, err := io.WriteString(w, l+"\n"); err != nil {
				return err
			}
		}
		return nil
	}
	lines := make([]string, len(rows))
	for i, r := range rows {
		lines[i] = strings.Join(r, "\t")
	}
	err = write("soi.tsv", strings.Join(xbrl.Header(), "\t"), lines)
	if err == nil {
		err = write("filings.tsv", "adsh\tcik\tname\tform\tfiled\tperiod\trows", manifest)
	}
	if cerr := zw.Close(); err == nil {
		err = cerr
	}
	if cerr := f.Close(); err == nil {
		err = cerr
	}
	if err != nil {
		os.Remove(tmp)
		return err
	}
	return os.Rename(tmp, dst)
}
