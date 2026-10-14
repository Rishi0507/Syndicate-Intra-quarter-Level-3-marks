package ident

import (
	"sort"
	"strings"
)

var industryLabels = []string{
	"Energy", "Materials", "Industrials", "Consumer Discretionary", "Consumer Staples", "Health Care", "Healthcare",
	"Financials", "Information Technology", "Communication Services", "Utilities", "Real Estate",
	"Capital Goods", "Commercial & Professional Services", "Transportation", "Automobiles & Components",
	"Consumer Durables & Apparel", "Consumer Services", "Retailing", "Consumer Discretionary Distribution & Retail",
	"Consumer Staples Distribution & Retail", "Food & Staples Retailing", "Food, Beverage & Tobacco",
	"Household & Personal Products", "Health Care Equipment & Services", "Healthcare Equipment & Services",
	"Pharmaceuticals, Biotechnology & Life Sciences", "Banks", "Diversified Financials", "Financial Services",
	"Insurance", "Software & Services", "Technology Hardware & Equipment", "Semiconductors & Semiconductor Equipment",
	"Telecommunication Services", "Media & Entertainment", "Equity Real Estate Investment Trusts",
	"Real Estate Management & Development",
	"Energy Equipment & Services", "Oil, Gas & Consumable Fuels", "Chemicals", "Construction Materials",
	"Containers & Packaging", "Metals & Mining", "Paper & Forest Products", "Aerospace & Defense", "Building Products",
	"Construction & Engineering", "Electrical Equipment", "Industrial Conglomerates", "Machinery",
	"Trading Companies & Distributors", "Commercial Services & Supplies", "Professional Services",
	"Air Freight & Logistics", "Passenger Airlines", "Airlines", "Marine Transportation", "Marine",
	"Ground Transportation", "Road & Rail", "Transportation Infrastructure", "Automobile Components",
	"Auto Components", "Automobiles", "Household Durables", "Leisure Products", "Textiles, Apparel & Luxury Goods",
	"Hotels, Restaurants & Leisure", "Diversified Consumer Services", "Distributors", "Broadline Retail",
	"Internet & Direct Marketing Retail", "Multiline Retail", "Specialty Retail", "Beverages", "Food Products",
	"Tobacco", "Household Products", "Personal Care Products", "Personal Products",
	"Health Care Equipment & Supplies", "Healthcare Equipment & Supplies", "Health Care Providers & Services",
	"Healthcare Providers & Services", "Health Care Technology", "Healthcare Technology", "Biotechnology",
	"Pharmaceuticals", "Life Sciences Tools & Services", "Thrifts & Mortgage Finance", "Diversified Financial Services",
	"Consumer Finance", "Capital Markets", "Mortgage Real Estate Investment Trusts", "IT Services", "Software",
	"Communications Equipment", "Technology Hardware, Storage & Peripherals",
	"Electronic Equipment, Instruments & Components", "Semiconductors", "Diversified Telecommunication Services",
	"Wireless Telecommunication Services", "Media", "Entertainment", "Interactive Media & Services",
	"Electric Utilities", "Gas Utilities", "Multi-Utilities", "Water Utilities",
	"Independent Power and Renewable Electricity Producers", "Diversified REITs", "Health Care REITs",
	"Industrial REITs", "Office REITs", "Residential REITs", "Retail REITs", "Specialized REITs",
	"Application Software", "Systems Software", "Asset Management & Custody Banks", "Insurance Brokers",
	"Health Care Services", "Healthcare Services", "Health Care Facilities", "Health Care Distributors",
	"Research & Consulting Services", "Human Resource & Employment Services", "Environmental & Facilities Services",
	"Diversified Support Services", "Education Services", "Specialized Consumer Services",
	"Industrial Machinery & Supplies & Components", "Electrical Components & Equipment", "Building Products",
	"Trucking", "Air Freight and Logistics", "Packaged Foods & Meats", "Specialty Chemicals", "Leisure Facilities",
	"Internet Software & Services", "Data Processing & Outsourced Services", "IT Consulting & Other Services",
	"Property & Casualty Insurance", "Multi-Sector Holdings", "Specialized Finance", "Other Diversified Financial Services",
	"Automotive", "Banking, Finance, Insurance & Real Estate", "Banking, Finance, Insurance and Real Estate",
	"Beverage, Food & Tobacco", "Capital Equipment", "Chemicals, Plastics & Rubber", "Construction & Building",
	"Consumer Goods: Durable", "Consumer Goods: Non-Durable", "Containers, Packaging & Glass", "Energy: Electricity",
	"Energy: Oil & Gas", "Environmental Industries", "Forest Products & Paper", "Healthcare & Pharmaceuticals",
	"High Tech Industries", "Hotel, Gaming & Leisure", "Media: Advertising, Printing & Publishing",
	"Media: Broadcasting & Subscription", "Media: Diversified & Production", "Retail", "Services: Business",
	"Services: Consumer", "Sovereign & Public Finance", "Telecommunications", "Transportation: Cargo",
	"Transportation: Consumer", "Utilities: Electric", "Utilities: Oil & Gas", "Utilities: Water", "Wholesale",
	"Business Services", "Consumer Products", "Consumer Goods", "Education", "Financial Services", "Food & Beverage",
	"Healthcare, Education and Childcare", "Health Care Providers and Services", "Finance and Insurance",
	"Manufacturing", "Technology", "Telecom", "Specialty Finance", "Digital Infrastructure & Services",
	"Infrastructure", "Distribution", "Internet Software and Services", "Leisure & Entertainment",
	"Buildings & Real Estate", "Cargo Transport", "Diversified/Conglomerate Service", "Diversified/Conglomerate Manufacturing",
	"Electronics", "Home and Office Furnishings, Housewares, and Durable Consumer Products", "Personal, Food and Miscellaneous Services",
	"Printing and Publishing", "Personal and Non Durable Consumer Products (Mfg. Only)", "Oil and Gas", "Insurance Services",
	"Artificial Intelligence and Automation", "Biotechnology Tools", "Drug Discovery & Development", "Medical Devices & Equipment",
	"Sustainable and Renewable Technology", "Consumer & Business Services", "Information Services", "Life Sciences",
}

var industryWords = func() [][]string {
	seen := map[string]bool{}
	var out [][]string
	for _, l := range industryLabels {
		w := words(l)
		k := strings.Join(w, " ")
		if len(w) > 0 && !seen[k] {
			seen[k] = true
			out = append(out, w)
		}
	}
	sort.SliceStable(out, func(i, j int) bool { return len(out[i]) > len(out[j]) })
	return out
}()

func words(s string) []string {
	s = strings.ReplaceAll(strings.ToLower(s), "&", " and ")
	return strings.FieldsFunc(s, func(r rune) bool {
		return !(r >= 'a' && r <= 'z' || r >= '0' && r <= '9')
	})
}

func wordEnds(s string, n int) []int {
	var ends []int
	in := false
	lower := strings.ToLower(s)
	for i := 0; i < len(lower) && len(ends) < n; i++ {
		c := lower[i]
		alnum := c >= 'a' && c <= 'z' || c >= '0' && c <= '9'
		if c == '&' {
			if in {
				ends = append(ends, i)
				in = false
			}
			if len(ends) < n {
				ends = append(ends, i+1)
			}
			continue
		}
		if alnum && !in {
			in = true
		} else if !alnum && in {
			ends = append(ends, i)
			in = false
		}
	}
	if in && len(ends) < n {
		ends = append(ends, len(s))
	}
	return ends
}

func stripIndustry(s string) (string, bool) {
	w := words(s)
	for _, ind := range industryWords {
		if len(ind) >= len(w) {
			continue
		}
		match := true
		for i, x := range ind {
			if w[i] != x {
				match = false
				break
			}
		}
		if match {
			ends := wordEnds(s, len(ind))
			if len(ends) == len(ind) {
				return strings.TrimLeft(s[ends[len(ends)-1]:], " ,:-"), true
			}
		}
	}
	return s, false
}

func isIndustry(seg string) bool {
	w := words(seg)
	for len(w) > 1 && isDigits(w[len(w)-1]) {
		w = w[:len(w)-1]
	}
	if len(w) == 0 {
		return false
	}
	for _, ind := range industryWords {
		if len(ind) != len(w) {
			continue
		}
		match := true
		for i := range ind {
			if ind[i] != w[i] {
				match = false
				break
			}
		}
		if match {
			return true
		}
	}
	return false
}

func isDigits(s string) bool {
	if s == "" {
		return false
	}
	for i := 0; i < len(s); i++ {
		if s[i] < '0' || s[i] > '9' {
			return false
		}
	}
	return true
}
