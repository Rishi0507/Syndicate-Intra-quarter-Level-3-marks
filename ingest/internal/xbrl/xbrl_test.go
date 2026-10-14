package xbrl

import (
	"strings"
	"testing"
)

const instance = `<?xml version="1.0" encoding="US-ASCII"?>
<xbrl xmlns="http://www.xbrl.org/2003/instance"
  xmlns:us-gaap="http://fasb.org/us-gaap/2026"
  xmlns:srt="http://fasb.org/srt/2026"
  xmlns:abc="http://example.com/20260630"
  xmlns:xbrldi="http://xbrl.org/2006/xbrldi"
  xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <context id="c-1">
    <entity>
      <identifier scheme="http://www.sec.gov/CIK">0000000001</identifier>
      <segment>
        <xbrldi:typedMember dimension="us-gaap:InvestmentIdentifierAxis">
          <us-gaap:limitedToken.domain>Acme Holdings, LLC |  First lien senior secured loan</us-gaap:limitedToken.domain>
        </xbrldi:typedMember>
        <xbrldi:explicitMember dimension="us-gaap:IndustrySectorAxis">abc:SoftwareSectorMember</xbrldi:explicitMember>
        <xbrldi:explicitMember dimension="us-gaap:InvestmentIssuerAffiliationAxis">us-gaap:InvestmentUnaffiliatedIssuerMember</xbrldi:explicitMember>
      </segment>
    </entity>
    <period><instant>2026-06-30</instant></period>
  </context>
  <context id="c-2">
    <entity>
      <identifier scheme="http://www.sec.gov/CIK">0000000001</identifier>
      <segment>
        <xbrldi:typedMember dimension="us-gaap:InvestmentIdentifierAxis">
          <us-gaap:limitedToken.domain>Acme Holdings, LLC |  First lien senior secured loan</us-gaap:limitedToken.domain>
        </xbrldi:typedMember>
        <xbrldi:explicitMember dimension="us-gaap:FairValueByFairValueHierarchyLevelAxis">us-gaap:FairValueInputsLevel3Member</xbrldi:explicitMember>
      </segment>
    </entity>
    <period><instant>2026-06-30</instant></period>
  </context>
  <context id="c-3">
    <entity>
      <identifier scheme="http://www.sec.gov/CIK">0000000001</identifier>
    </entity>
    <period><instant>2026-06-30</instant></period>
  </context>
  <us-gaap:InvestmentOwnedAtFairValue contextRef="c-1" decimals="-3" unitRef="usd">9850000</us-gaap:InvestmentOwnedAtFairValue>
  <us-gaap:InvestmentOwnedAtFairValue contextRef="c-1" decimals="-3" unitRef="usd">1</us-gaap:InvestmentOwnedAtFairValue>
  <us-gaap:InvestmentOwnedBalancePrincipalAmount contextRef="c-1" decimals="-3" unitRef="usd">10000000</us-gaap:InvestmentOwnedBalancePrincipalAmount>
  <us-gaap:InvestmentBasisSpreadVariableRate contextRef="c-1" decimals="4" unitRef="pure">0.0525</us-gaap:InvestmentBasisSpreadVariableRate>
  <us-gaap:InvestmentMaturityDate contextRef="c-1">2030-03-15</us-gaap:InvestmentMaturityDate>
  <us-gaap:InvestmentOwnedAtCost contextRef="c-1" xsi:nil="true" unitRef="usd"/>
  <us-gaap:InvestmentOwnedAtFairValue contextRef="c-2" decimals="-3" unitRef="usd">9850000</us-gaap:InvestmentOwnedAtFairValue>
  <us-gaap:InvestmentOwnedAtFairValue contextRef="c-3" decimals="-3" unitRef="usd">500000000</us-gaap:InvestmentOwnedAtFairValue>
  <abc:InvestmentOwnedAtFairValue contextRef="c-1" decimals="-3" unitRef="usd">7</abc:InvestmentOwnedAtFairValue>
</xbrl>`

const labels = `<?xml version="1.0" encoding="utf-8"?>
<link:linkbase xmlns:link="http://www.xbrl.org/2003/linkbase" xmlns:xlink="http://www.w3.org/1999/xlink">
  <link:labelLink xlink:type="extended" xlink:role="http://www.xbrl.org/2003/role/link">
    <link:loc xlink:type="locator" xlink:href="abc-20260630.xsd#abc_SoftwareSectorMember" xlink:label="loc_1"/>
    <link:label xlink:type="resource" xlink:label="lab_1" xlink:role="http://www.xbrl.org/2003/role/terseLabel" xml:lang="en-US">Software</link:label>
    <link:label xlink:type="resource" xlink:label="lab_1" xlink:role="http://www.xbrl.org/2003/role/label" xml:lang="en-US">Software Sector [Member]</link:label>
    <link:labelArc xlink:type="arc" xlink:from="loc_1" xlink:to="lab_1"/>
  </link:labelLink>
</link:linkbase>`

func rows(t *testing.T) []Row {
	t.Helper()
	in, err := ParseInstance(strings.NewReader(instance))
	if err != nil {
		t.Fatal(err)
	}
	names, err := ParseLabels(strings.NewReader(labels))
	if err != nil {
		t.Fatal(err)
	}
	return in.Rows(Filing{Adsh: "0000000001-26-000001", CIK: "1", Name: "ACME BDC", Form: "10-Q", Filed: "2026-08-10", Period: "2026-06-30"}, names)
}

func column(name string) int {
	for i, h := range Header() {
		if h == name {
			return i
		}
	}
	return -1
}

func TestRowsFollowTheDataSetLayout(t *testing.T) {
	got := rows(t)
	if len(got) != 2 {
		t.Fatalf("got %d rows, want one per context that carries an identifier", len(got))
	}
	r := got[0]
	if len(r) != len(Header()) {
		t.Fatalf("row has %d fields, header %d", len(r), len(Header()))
	}
	want := map[string]string{
		"adsh":                                  "0000000001-26-000001",
		"ddate":                                 "2026-06-30",
		"filed":                                 "2026-08-10",
		"InvestmentIdentifierAxis":              "Acme Holdings, LLC |  First lien senior secured loan",
		"IndustrySectorAxis":                    "Software Sector [Member]",
		"InvestmentIssuerAffiliationAxis":       "Investment Unaffiliated Issuer",
		"InvestmentOwnedAtFairValue":            "9850000",
		"InvestmentOwnedBalancePrincipalAmount": "10000000",
		"InvestmentBasisSpreadVariableRate":     "0.0525",
		"InvestmentMaturityDate":                "2030-03-15",
		"InvestmentOwnedAtCost":                 "",
		"OtherAxes":                             "",
	}
	for k, v := range want {
		if got := r[column(k)]; got != v {
			t.Errorf("%s = %q, want %q", k, got, v)
		}
	}
	if o := got[1][column("OtherAxes")]; o != "FairValueByFairValueHierarchyLevelAxis=Fair Value Inputs Level3" {
		t.Errorf("second context keeps its extra axis, got %q", o)
	}
}

func TestHumanize(t *testing.T) {
	for in, want := range map[string]string{
		"InvestmentAffiliatedIssuerControlledMember": "Investment Affiliated Issuer Controlled",
		"DebtSecuritiesMember":                       "Debt Securities",
		"USDollarMember":                             "US Dollar",
	} {
		if got := humanize(in); got != want {
			t.Errorf("humanize(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestLatinOneLabels(t *testing.T) {
	doc := "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>\n" + strings.Replace(labels, "Software Sector [Member]", "Caf\xe9 Sector [Member]", 1)[len("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n"):]
	names, err := ParseLabels(strings.NewReader(doc))
	if err != nil {
		t.Fatal(err)
	}
	if got := names["abc_SoftwareSectorMember"]; got != "Café Sector [Member]" {
		t.Errorf("label = %q", got)
	}
}
