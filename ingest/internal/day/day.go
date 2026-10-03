// Package day is a small calendar-date type. Everything downstream of ingest
// keys on whole days, so dates are carried as yyyymmdd integers and converted
// to a serial day number only where arithmetic is needed.
package day

import (
	"fmt"
	"strconv"
	"strings"
	"time"
)

// Date is a calendar date encoded as yyyymmdd. The zero value means "unknown".
type Date int32

// New builds a Date from its components without validating them.
func New(y, m, d int) Date { return Date(y*10000 + m*100 + d) }

func (d Date) Year() int    { return int(d) / 10000 }
func (d Date) Month() int   { return int(d) / 100 % 100 }
func (d Date) Day() int     { return int(d) % 100 }
func (d Date) IsZero() bool { return d == 0 }

func (d Date) Time() time.Time {
	return time.Date(d.Year(), time.Month(d.Month()), d.Day(), 0, 0, 0, 0, time.UTC)
}

// Serial is the number of days since 1970-01-01.
func (d Date) Serial() int { return int(d.Time().Unix() / 86400) }

func FromTime(t time.Time) Date { return New(t.Year(), int(t.Month()), t.Day()) }

func FromSerial(n int) Date { return FromTime(time.Unix(int64(n)*86400, 0).UTC()) }

func (d Date) AddDays(n int) Date { return FromSerial(d.Serial() + n) }

func (d Date) String() string {
	if d.IsZero() {
		return ""
	}
	return fmt.Sprintf("%04d-%02d-%02d", d.Year(), d.Month(), d.Day())
}

// MonthEnd returns the last day of d's month.
func (d Date) MonthEnd() Date {
	first := time.Date(d.Year(), time.Month(d.Month()), 1, 0, 0, 0, 0, time.UTC)
	return FromTime(first.AddDate(0, 1, -1))
}

// Parse accepts the three spellings the SEC data sets use across tables:
// mm/dd/yyyy, yyyy-mm-dd and yyyymmdd. Anything else yields the zero Date.
func Parse(s string) Date {
	s = strings.TrimSpace(s)
	if s == "" {
		return 0
	}
	if len(s) >= 10 && s[4] == '-' && s[7] == '-' {
		return build(s[0:4], s[5:7], s[8:10])
	}
	if strings.Count(s, "/") == 2 {
		p := strings.SplitN(s, "/", 3)
		// Tolerate a trailing time component after the year.
		if i := strings.IndexByte(p[2], ' '); i > 0 {
			p[2] = p[2][:i]
		}
		return build(p[2], p[0], p[1])
	}
	if len(s) == 8 {
		return build(s[0:4], s[4:6], s[6:8])
	}
	return 0
}

func build(ys, ms, ds string) Date {
	y, e1 := strconv.Atoi(ys)
	m, e2 := strconv.Atoi(ms)
	d, e3 := strconv.Atoi(ds)
	if e1 != nil || e2 != nil || e3 != nil {
		return 0
	}
	if y < 1900 || y > 2200 || m < 1 || m > 12 || d < 1 || d > 31 {
		return 0
	}
	return New(y, m, d)
}
