// Package fetch downloads the public inputs: the SEC's BDC data sets and
// FRED market series.
package fetch

import (
	"archive/zip"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"time"
)

const (
	secIndex = "https://www.sec.gov/data-research/sec-markets-data/bdc-data-sets"
	secHost  = "https://www.sec.gov"
	// The SEC's fair access policy allows ten requests a second. One a second
	// is plenty for a couple of dozen files.
	secPause = time.Second
)

var bdcZipRe = regexp.MustCompile(`href="([^"]*?([0-9]{4}(?:q[1-4]|_[0-9]{2})_bdc\.zip))"`)

// ErrNoContact is returned when no contact has been supplied for the SEC.
var ErrNoContact = errors.New(`the SEC requires automated clients to identify themselves.
Set SEC_CONTACT to a name and an email address you can be reached at, for example

    $env:SEC_CONTACT = "Jane Doe jane@example.com"

and run this again. The value is sent as the User-Agent on requests to sec.gov
and nowhere else. See https://www.sec.gov/os/accessing-edgar-data`)

type secClient struct {
	http *http.Client
	ua   string
	last time.Time
}

func (c *secClient) get(url string) (*http.Response, error) {
	if wait := secPause - time.Since(c.last); wait > 0 {
		time.Sleep(wait)
	}
	c.last = time.Now()
	req, err := http.NewRequest(http.MethodGet, url, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("User-Agent", c.ua)
	req.Header.Set("Accept-Encoding", "identity")
	resp, err := c.http.Do(req)
	if err != nil {
		return nil, err
	}
	if resp.StatusCode == http.StatusForbidden {
		resp.Body.Close()
		return nil, fmt.Errorf("sec.gov refused the request (403). It rejects clients whose User-Agent does not carry a reachable contact; check SEC_CONTACT")
	}
	if resp.StatusCode != http.StatusOK {
		resp.Body.Close()
		return nil, fmt.Errorf("GET %s: %s", url, resp.Status)
	}
	return resp, nil
}

// SEC downloads every BDC data set archive listed on the SEC's index page
// into dir, skipping any that are already present and intact. The contact is
// declared in the User-Agent exactly as the SEC asks; the function refuses to
// run without one rather than presenting itself as something it is not.
func SEC(dir, contact string, logf func(string, ...any)) ([]string, error) {
	contact = strings.TrimSpace(contact)
	if contact == "" || !strings.Contains(contact, "@") {
		return nil, ErrNoContact
	}
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return nil, err
	}
	c := &secClient{http: &http.Client{Timeout: 10 * time.Minute}, ua: "Syndicate-Research " + contact}

	resp, err := c.get(secIndex)
	if err != nil {
		return nil, err
	}
	page, err := io.ReadAll(io.LimitReader(resp.Body, 8<<20))
	resp.Body.Close()
	if err != nil {
		return nil, err
	}
	seen := map[string]string{}
	for _, m := range bdcZipRe.FindAllStringSubmatch(string(page), -1) {
		href, name := m[1], m[2]
		if strings.HasPrefix(href, "/") {
			href = secHost + href
		}
		seen[name] = href
	}
	if len(seen) == 0 {
		return nil, fmt.Errorf("no data set links found on %s; the page layout may have changed", secIndex)
	}
	names := make([]string, 0, len(seen))
	for n := range seen {
		names = append(names, n)
	}
	sort.Strings(names)

	var got []string
	for _, name := range names {
		dst := filepath.Join(dir, name)
		if intact(dst) {
			logf("have %s", name)
			got = append(got, dst)
			continue
		}
		logf("fetching %s", name)
		if err := c.download(seen[name], dst); err != nil {
			return got, fmt.Errorf("%s: %w", name, err)
		}
		got = append(got, dst)
	}
	return got, nil
}

func (c *secClient) download(url, dst string) error {
	resp, err := c.get(url)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	tmp := dst + ".part"
	f, err := os.Create(tmp)
	if err != nil {
		return err
	}
	if _, err := io.Copy(f, resp.Body); err != nil {
		f.Close()
		os.Remove(tmp)
		return err
	}
	if err := f.Close(); err != nil {
		os.Remove(tmp)
		return err
	}
	if !intact(tmp) {
		os.Remove(tmp)
		return errors.New("downloaded file is not a readable zip archive")
	}
	return os.Rename(tmp, dst)
}

// intact reports whether path is a zip archive that opens. A half-written
// download from an interrupted run fails this and is fetched again.
func intact(path string) bool {
	zr, err := zip.OpenReader(path)
	if err != nil {
		return false
	}
	zr.Close()
	return true
}
