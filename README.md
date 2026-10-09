# Syndicate

**Intra-quarter Level 3 marks for private credit.**

Syndicate estimates the fair value of private loans on every day between quarterly marks, attaches a
calibrated confidence band to each estimate, and converts that uncertainty into an indicative
two-sided quote. It runs on the loan-level schedules of investments that business development
companies (BDCs) file with the SEC.

> **Disclaimer.** Syndicate is independent research built from public filings and does not
> constitute investment advice. Lender names appear because they are disclosed in those filings.
> Figures shown beside them are statistical estimates and are not findings about any manager's
> valuation process.

## Background

Private credit is increasingly expected to offer investors daily pricing and, over time, a secondary
market. Large managers have announced daily valuations for private assets and have begun making
markets in them. Almost none of the underlying loans has ever traded.

These loans are Level 3 assets: their fair value rests on unobservable inputs. Each lender reports a
mark at quarter end, and the filing becomes public several weeks later. Between filings there is no
observed price.

Quoting such a loan requires two numbers every day: an estimate of value, and a measure of
confidence in that estimate. The second number sets the adverse-selection component of a dealer's
spread and is rarely available. Syndicate produces both.

The approach reads each loan through the lenders that hold it. When several lenders hold the same
loan, their marks for the same reporting date differ, and those differences carry information about
the loan and about each lender's marking behaviour. The name refers to that structure: one loan,
held by a syndicate of lenders, each of which marks it independently.

## Data

The SEC's [BDC data sets](https://www.sec.gov/data-research/sec-markets-data/bdc-data-sets) carry
loan-level marks for every business development company from late 2022. The SEC requires automated
clients to identify themselves with a contact address, so the downloader does not run without one
and does not present itself as a browser:

```sh
go build -C ingest -o ../bin/ingest ./cmd/ingest
export SEC_CONTACT="Your Name you@example.com"
./bin/ingest fetch-sec -out data/raw/sec
./bin/ingest fetch-fred -out data/raw/fred
./bin/ingest build -sec data/raw/sec -fred data/raw/fred -out data/build
```

Alternatively, download the `*_bdc.zip` archives manually into `data/raw/sec` and run only the
`fetch-fred` and `build` steps.

The repository contains no data. The archives and the panel are generated locally under `data/`.

The filings require substantial cleaning, and most of the ingest code exists for that purpose.
Published column names do not match the columns that hold the values. Four families of free-text
identifier are in use, some with the first letter of the borrower missing and some with schedule
headings run into the name. Unfunded commitments resemble loans marked at zero, euro-denominated
loans resemble loans marked at 108, and a small number of rows are a thousand times too large. A
single borrower and seniority can conceal a first-out and a last-out piece, or new paper issued in a
restructuring. Each case is handled by an explicit rule, and `data/build/ingest_report.json` counts
the effect of every rule on every row. Of 1.84 million rows, 851,299 restate an earlier date in a
later filing, 662,326 are dropped and 328,783 are kept.
