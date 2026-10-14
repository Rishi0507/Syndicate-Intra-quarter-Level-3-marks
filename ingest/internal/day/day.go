package day

import (
	"fmt"
	"strconv"
	"strings"
	"time"
)

type Date int32

func New(y, m, d int) Date { return Date(y*10000 + m*100 + d) }

func (d Date) Year() int    { return int(d) / 10000 }
func (d Date) Month() int   { return int(d) / 100 % 100 }
func (d Date) Day() int     { return int(d) % 100 }
func (d Date) IsZero() bool { return d == 0 }

func (d Date) Time() time.Time {
	return time.Date(d.Year(), time.Month(d.Month()), d.Day(), 0, 0, 0, 0, time.UTC)
}

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

func (d Date) MonthEnd() Date {
	first := time.Date(d.Year(), time.Month(d.Month()), 1, 0, 0, 0, 0, time.UTC)
	return FromTime(first.AddDate(0, 1, -1))
}

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
