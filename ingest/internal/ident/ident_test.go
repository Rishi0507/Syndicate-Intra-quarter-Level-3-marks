package ident

import "testing"

func TestParse(t *testing.T) {
	cases := []struct {
		id, typ  string
		borrower string
		lien     Lien
		facility Facility
	}{
		{"Associations, Inc., First lien senior secured loan", "", "Associations, Inc.", FirstLien, Term},
		{"1261229 B.C. LTD., First lien senior secured notes", "", "1261229 B.C. LTD.", FirstLien, Term},
		{"Acme Buyer, LLC, First lien senior secured revolving loan", "", "Acme Buyer, LLC", FirstLien, Revolver},
		{"Harbor Logistics Corp., Second lien senior secured loan", "", "Harbor Logistics Corp.", SecondLien, Term},
		{"Access CIG, LLC, First Lien Term Loan", "", "Access CIG, LLC", FirstLien, Term},
		{"AAC Holdings, Inc., Secured Debt 1", "", "AAC Holdings, Inc.", FirstLien, Term},

		{"AAH TOPCO, LLC | One stop 1 | Non-Affiliated Issuer", "", "AAH TOPCO, LLC", Unitranche, Term},
		{"Apothecary Products, LLC | Senior secured 2 | Non-Affiliated Issuer", "", "Apothecary Products, LLC", FirstLien, Term},
		{"Bristol Hospice L.L.C. | First lien senior secured revolving loan | Non-Affiliated Issuer", "", "Bristol Hospice L.L.C.", FirstLien, Revolver},
		{"Community Care Partners, LLC | First lien senior secured delayed draw term loan", "", "Community Care Partners, LLC", FirstLien, DelayedDraw},
		{"Breeze Aviation Group Inc | Second Lien Senior Secured Term Loan 3", "", "Breeze Aviation Group Inc", SecondLien, Term},
		{"601 Lafayette Mezz LLC (Delayed Draw) | Junior Secured Loans", "", "601 Lafayette Mezz LLC", SecondLien, DelayedDraw},
		{"Brandner Design, LLC | Revolving Loan 2", "", "Brandner Design, LLC", LienUnknown, Revolver},

		{"123Dentist, Inc. 1 | Non-Affiliated Issuer", "", "123Dentist, Inc.", LienUnknown, Term},
		{"Cloud Software Group Inc 2 | Non-Affiliated Issuer", "", "Cloud Software Group Inc", LienUnknown, Term},
		{"365 Retail Markets, LLC 1", "", "365 Retail Markets, LLC", LienUnknown, Term},

		{"3Pillar Global Inc | Software & Services 1", "", "3Pillar Global Inc", LienUnknown, Term},
		{"CSafe Global | Transportation 3", "", "CSafe Global", LienUnknown, Term},
		{"Advanced Dermatology & Cosmetic Surgery | Health Care Equipment & Services", "", "Advanced Dermatology & Cosmetic Surgery", LienUnknown, Term},
		{"Center Phase Energy, LLC | Electric Utilities 1", "", "Center Phase Energy, LLC", LienUnknown, Term},

		{"Funded Investments - First lien | Accelya Lux Finco S.a r.l.", "", "Accelya Lux Finco S.a r.l.", FirstLien, Term},
		{"US Corporate Debt | 1st Lien/Senior Secured Debt | Airwavz Solutions Inc. | Digital Infrastructure & Services | Term Loan | 9.01% (S + 5.25%; 1.00% Floor) | 3/31/2027", "", "Airwavz Solutions Inc.", FirstLien, Term},
		{"Non-Controlled, Non-Affiliated Debt Investments | First Lien Senior Secured | Industrial Machinery & Supplies & Components | Rapid Pump Acquisition, Inc. Delayed Draw Term C Loan | Reference Rate Spread / Floor | 1M SOFR + 5.75% / 1.00%", "", "Rapid Pump Acquisition, Inc.", FirstLien, DelayedDraw},

		{"Investments - non-controlled/non-affiliated Secured Debt Air Freight and Logistics Lightbeam Bidco Inc. Asset Type First Lien Delayed Draw Term Loan Reference Rate and Spread S + 4.75% Interest Rate 8.42% Maturity Date 05/06/2030 One", "", "Lightbeam Bidco Inc.", FirstLien, DelayedDraw},
		{"Investments-non-controlled/non-affiliated Debt Investments Beverages Vamos Bidco, Inc. Investment First Lien Debt Reference Rate & Spread S + 4.75% Interest Rate 8.45% Maturity Date 01/30/2032 One", "", "Vamos Bidco, Inc.", FirstLien, Term},
		{"Debt Investments - non-controlled/non-affiliated Commercial Services & Supplies Consor Intermediate II, LLC Investment First Lien Debt Reference Rate and Spread S + 4.50% Interest Rate 8.20% Maturity Date 12/5/2031 Two", "", "Consor Intermediate II, LLC", FirstLien, Term},
		{"Aerospace & Defense Accel International Accel International Holdings, LLC Investment Type First Lien Secured Debt - Revolver Interest Rate S+450, 0.50% Floor Maturity Date 04/26/2032", "", "Accel International Holdings, LLC", FirstLien, Revolver},
		{"Communications Equipment MCA Mobile Communications America, Inc. First Lien Secured Debt - Term Loan SOFR+475, 1.00% Floor Maturity Date 10/16/29", "", "MCA Mobile Communications America, Inc.", FirstLien, Term},
		{"Aerospace & Defense ATS First Lien Senior Secured Loan SOFR Spread 5.75% Interest Rate 9.42% Maturity Date 7/12/2029", "", "ATS", FirstLien, Term},
		{"Services: Business Discovery Senior Living First Lien Senior Secured Loan - Delayed Draw SOFR Spread 4.75% Interest Rate 8.42% Maturity Date 3/18/2030", "", "Discovery Senior Living", FirstLien, DelayedDraw},
		{"Debt Investments Aerospace and Defense Arcline FM Holdings, LLC (Fairbanks Morse) Instrument First Lien Term Loan Ref SOFR(Q) Floor 0.75% Spread 2.75% Total Coupon 6.45% Maturity 6/23/2030", "", "Arcline FM Holdings, LLC", FirstLien, Term},
		{"Investments United States Debt Investments Commercial & Professional Services Guardian Access Solutions Investment Type Senior Secured First Lien Delayed Draw Term Loan Interest Term S + 600, 100 Floor Interest Rate 9.67%", "", "Guardian Access Solutions", FirstLien, DelayedDraw},
		{"Non-Controlled/Non-Affiliated Portfolio Company Investments First Lien Debt Investments Health Care Equipment & Supplies Perkinelmer U.S. LLC Investment Type First Lien Term Loan Reference Rate and Spread S + 4.75% Maturity Date 3/13/2029", "", "Perkinelmer U.S. LLC", FirstLien, Term},
		{"Portfolio Company Debt Securities- United States Finance and Insurance Gravie, Inc. Type of Investment Secured Loan Investment Date June 4, 2024 Maturity Date July 1, 2029", "", "Gravie, Inc.", FirstLien, Term},
		{"Affiliate Investments Autonomy Data Services, Inc.", "", "Autonomy Data Services, Inc.", LienUnknown, Term},

		{"Investment Debt Investments - 179.3% United States - 161.1% 1st Lien/Senior Secured Debt - 155.1% Avalara, Inc Industry Software Interest Rate 6.45% Reference Rate and Spread S + 2.75% Maturity 03/26/32", "", "Avalara, Inc", FirstLien, Term},
		{"Affiliate investments - 13.4% - Axero Holdings, LLC - Employee Collaboration Software - Delayed Draw Term Loan 4.50% Cash, (3M USD TERM SOFR + 2.50%) PIK, 12/31/2027", "", "Axero Holdings, LLC", LienUnknown, DelayedDraw},

		{"First Lien Secured Debt Issuer Name Kinetic Purchaser, LLC Acquisition 11/30/2021 Maturity 11/10/2027 Industry Personal Products", "", "Kinetic Purchaser, LLC", FirstLien, Term},
		{"Equity Securities Issuer Name 48Forty Intermediate Holdings, Inc. - Common Equity Acquisition 11/5/2024 Industry Containers and Packaging", "", "48Forty Intermediate Holdings, Inc.", Equity, Term},

		{"Cascade Foods, Inc., Term Loan B", "DebtSecuritiesSecondLienMember", "Cascade Foods, Inc.", SecondLien, Term},
		{"Ironwood Energy Services, LLC, Senior unsecured notes", "", "Ironwood Energy Services, LLC", Subordinated, Term},

		{"Pluralsight, LLC, Common Stock", "", "Pluralsight, LLC", Equity, Term},
		{"Capital Integration Systems LLC (dba CAIS) | Class D Common Units | Non-Affiliated", "", "Capital Integration Systems LLC", Equity, Term},
		{"BROOKP 2024-1, Collaterized loan obligation", "", "BROOKP 2024-1", Structured, Term},
		{"Bain Capital Credit CLO 2022-1, Ltd - Class ER | Non-Affiliated Issuer", "", "Bain Capital Credit CLO 2022-1, Ltd", Structured, Term},
		{"Ballyrock CLO 32, Ltd. | Structured Finance Note", "", "Ballyrock CLO 32, Ltd.", Structured, Term},
		{"Orchard Senior Loan Fund LLC, Joint Venture", "Investment Funds [Member]", "Orchard Senior Loan Fund LLC", Structured, Term},

		{"Prime Loan Servicing Group, First lien senior secured loan", "", "Prime Loan Servicing Group", FirstLien, Term},
		{"Sofranko Industrial Products Inc First lien senior secured loan", "", "Sofranko Industrial Products Inc", FirstLien, Term},
		{"Parexel International Corp., First lien senior secured loan", "", "Parexel International Corp.", FirstLien, Term},
	}
	for _, c := range cases {
		p := Parse(c.id, c.typ)
		if p.Borrower != c.borrower || p.Lien != c.lien || p.Facility != c.facility {
			t.Errorf("Parse(%q, %q)\n  got  borrower=%q lien=%s facility=%s\n  want borrower=%q lien=%s facility=%s",
				c.id, c.typ, p.Borrower, p.Lien, p.Facility, c.borrower, c.lien, c.facility)
		}
	}
}

func TestAliases(t *testing.T) {
	for id, want := range map[string]string{
		"Keystone Brands, LLC (f/k/a Old Keystone, Inc.), First lien senior secured loan":                "Old Keystone, Inc.",
		"Capital Integration Systems LLC (dba CAIS) | Class D Common Units | Non-Affiliated":             "CAIS",
		"360 Holdco, Inc. (360 Training) | First Lien Debt (Delayed Draw)":                               "360 Training",
		"Debt Investments Aerospace and Defense Arcline FM Holdings, LLC (Fairbanks Morse) Instrument X": "Fairbanks Morse",
	} {
		p := Parse(id, "")
		if len(p.Aliases) != 1 || p.Aliases[0] != want {
			t.Errorf("Parse(%q).Aliases = %q, want [%q]", id, p.Aliases, want)
		}
	}
}

func TestAuditedShapes(t *testing.T) {
	for id, want := range map[string]string{
		"ortfolio Company Debt Securities- United States Food Products Athletic Brewing Company, LLCType of Investment Equipment Financing":                                       "Athletic Brewing Company, LLC",
		"on-controlled/Non-affiliated investments Debt Investments Chemicals, Plastics & Rubber Atlas Intermediate III, L.L.C. 2022 Incremental Term Loan":                        "Atlas Intermediate III, L.L.C.",
		"APCF SPV I, LLC BANK LOANS: NON-CONTROL/NON-AFFILIATE INVESTMENTS Health Care Providers & Services National Seating & Mobility Investment Type Senior Secured Term Loan": "National Seating & Mobility",
		"ortfolio Investments Audax Credit BDC, Inc. BANK LOANS: NON-CONTROL/NON-AFFILIATE INVESTMENTS Capital Equipment Ohio Transmission Investment Type Senior Secured":        "Ohio Transmission",
		"Term Loan, United States of America, LAC Acquisition LLC d/b/a Lighthouse Autism Center, Healthcare Providers & Services, Rate S+11.10%, 2.50% Floor":                    "LAC Acquisition LLC",
		"Term Loan, United States of America, BayMark Health Services Inc, Healthcare Providers & Services, Rate S+8.76%, 1.00% Floor":                                            "BayMark Health Services Inc",
		"Type Non-control/Non-affiliate Investment Debt and Equity Investments BCPE North Star US Holdco 2, Inc. (F/K/A Dessert Holdings":                                         "BCPE North Star US Holdco 2, Inc.",
		"Senior Care Centers, LLC, First lien senior secured loan":                                                                                                                "Senior Care Centers, LLC",
	} {
		if got := Parse(id, "").Borrower; got != want {
			t.Errorf("Parse(%q).Borrower\n  got  %q\n  want %q", id, got, want)
		}
	}
}

func TestNameless(t *testing.T) {
	for id, want := range map[string]bool{
		"Senior Secured":                       true,
		"First Lien Term Loan":                 true,
		"Revolver":                             true,
		"Software & Services":                  true,
		"Largest Portfolio Company Investment": true,
		"Applied Systems, Inc., First lien senior secured loan": false,
		"ATS First Lien Senior Secured Loan":                    false,
	} {
		if got := Parse(id, "").Nameless(); got != want {
			t.Errorf("Parse(%q).Nameless() = %v, want %v (borrower %q)", id, got, want, Parse(id, "").Borrower)
		}
	}
}

func TestIndustryCaptured(t *testing.T) {
	for id, want := range map[string]string{
		"3Pillar Global Inc | Software & Services 1": "Software & Services",
		"Aerospace & Defense ATS First Lien Senior Secured Loan SOFR Spread 5.75% Interest Rate 9.42% Maturity Date 7/12/2029":         "Aerospace & Defense",
		"Term Loan, United States of America, BayMark Health Services Inc, Healthcare Providers & Services, Rate S+8.76%, 1.00% Floor": "Healthcare Providers & Services",
		"Applied Systems, Inc., First lien senior secured loan":                                                                        "",
	} {
		if got := Parse(id, "").Industry; got != want {
			t.Errorf("Parse(%q).Industry = %q, want %q", id, got, want)
		}
	}
}

func TestUnfunded(t *testing.T) {
	for id, want := range map[string]bool{
		"Investments  non-controlled/non-affiliated First Lien Debt AA&D Midco, Inc. Commitment Type Revolver Commitment Expiration Date 11/29/2030": true,
		"Coupa Holdings, LLC | First Lien - Undrawn 2":          true,
		"AAH Topco, LLC | First Lien - Drawn":                   false,
		"Applied Systems, Inc., First lien senior secured loan": false,
	} {
		if got := Parse(id, "").Unfunded; got != want {
			t.Errorf("Parse(%q).Unfunded = %v, want %v", id, got, want)
		}
	}
}

func TestHolderHeading(t *testing.T) {
	p := ParseFor("Portfolio Investments Audax Credit BDC, Inc. BANK LOANS: NON-CONTROL/NON-AFFILIATE INVESTMENTS Aerospace & Defense API Technologies Investment Type Senior Secured Term B-1 Loan (First Lien) Index S+ Spread 1.00%",
		"", "Audax Credit BDC, Inc.")
	if p.Borrower != "API Technologies" || p.Lien != FirstLien {
		t.Errorf("got %q %s", p.Borrower, p.Lien)
	}
}

func TestIndustries(t *testing.T) {
	for in, want := range map[string]string{
		"Aerospace & Defense Acme Holdings":            "Acme Holdings",
		"Aerospace and Defense Acme Holdings":          "Acme Holdings",
		"Health Care Equipment & Supplies Perkinelmer": "Perkinelmer",
		"Services: Business Discovery Senior Living":   "Discovery Senior Living",
		"Software":               "Software",
		"Softwarehouse Partners": "Softwarehouse Partners",
	} {
		if got, _ := stripIndustry(in); got != want {
			t.Errorf("stripIndustry(%q) = %q, want %q", in, got, want)
		}
	}
	if !isIndustry("Software & Services 2") || isIndustry("3Pillar Global Inc") {
		t.Error("isIndustry misjudged a segment")
	}
}

func TestDebtClasses(t *testing.T) {
	for l, want := range map[Lien]bool{LienUnknown: true, FirstLien: true, Unitranche: true,
		SecondLien: true, Subordinated: true, Equity: false, Structured: false} {
		if l.IsDebt() != want {
			t.Errorf("%s.IsDebt() = %v", l, !want)
		}
	}
}
