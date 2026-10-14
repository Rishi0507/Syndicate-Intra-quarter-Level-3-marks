import katex from "katex";
import "katex/dist/katex.min.css";
import { useMemo } from "react";
import { BRAND } from "../brand";
import type { Universe } from "../lib/data";
import { fmtInt, fmtPct } from "../lib/format";

function Tex({ children, block = false }: { children: string; block?: boolean }) {
  const html = useMemo(() => katex.renderToString(children, { displayMode: block, throwOnError: false, strict: false }), [children, block]);
  return block ? <div dangerouslySetInnerHTML={{ __html: html }} /> : <span dangerouslySetInnerHTML={{ __html: html }} />;
}

const T = String.raw;

export function Method({ u }: { u: Universe }) {
  const fit = u.meta.fit;
  const d = fit.dynamics;
  const q = Math.sqrt(fit.days_per_quarter);
  const synthetic = u.meta.source === "synthetic";

  return (
    <div className="page">
      <section className="glass pane">
        <p className="pull">Two numbers are missing from private credit: what a loan is worth today, and how sure anyone is.</p>
        <div className="prose" style={{ marginTop: 26 }}>
          <p>
            Private credit is being asked to do something it was not built for: to offer investors a price every day and, in
            time, to trade. For anything to trade, someone has to quote it. To quote a loan that has never traded you need two
            numbers on every day, not once a quarter: what it is worth, and how sure you are. The second number is the one
            usually missing, and it is the one a dealer's spread is made of.
          </p>
          <p>
            {BRAND} produces both, for the loans that business development companies report to the SEC. It reads each loan
            through the lenders that hold it. Two lenders mark the same loan, on the same date, a little differently, and that
            difference is information: about the loan, and about each of them.
          </p>
        </div>
      </section>

      <section className="glass pane">
        <header>
          <h2>The model</h2>
        </header>
        <div className="prose">
          <h3>A value that moves every day</h3>
          <p>
            A loan's fair value <Tex>{T`v_t`}</Tex>, as a fraction of par, is never observed. It moves each trading day with public
            credit markets, with a shock shared by its sector, and with news of its own:
          </p>
          <Tex block>{T`v_t = v_{t-1} + \mu(v_{t-1}) + \beta(v_{t-1})\,x_t + s_t + u_t, \qquad s_t \sim \mathcal N(0,\sigma_s^2),\quad u_t \sim \mathcal N\!\big(0,\sigma_u(v_{t-1})^2\big)`}</Tex>
          <p>
            Here <Tex>{T`x_t`}</Tex> is the day's fall in a public credit spread. Sensitivity and volatility both rise as a loan
            becomes stressed, <Tex>{T`\beta(v) = \beta_0 + \beta_1 (\bar v - v)^+`}</Tex> and{" "}
            <Tex>{T`\sigma_u(v) = \sigma_0 + \sigma_1(\bar v - v)^+`}</Tex> with <Tex>{T`\bar v = 0.97`}</Tex>. The drift{" "}
            <Tex>{T`\mu`}</Tex> pulls a performing loan back toward where performing loans settle, at one rate from below and
            another from above, and gives stressed loans a drift of their own.
          </p>

          <h3>A measurement that arrives late, from a lender with habits</h3>
          <p>
            Lender <Tex>{T`h`}</Tex> marks the loan on a reporting date <Tex>{T`a`}</Tex>, but the mark becomes public only when
            the filing lands, weeks later. And a mark is not fair value plus noise. Two lenders a point apart on a loan this
            quarter are mostly still a point apart the next: a lender's distance from fair value is a habit. Its view of the loan
            is
          </p>
          <Tex block>{T`\tilde m_{h,q} = v_{a_q} + b_h + d_{h,q}, \qquad d_{h,q} = \alpha\, d_{h,q-1} + \varepsilon_{h,q}, \quad \varepsilon_{h,q} \sim \mathcal N\!\big(0,\; (1-\alpha^2)\, r_h^2\, g(v)^2\big)`}</Tex>
          <p>
            with a level bias <Tex>{T`b_h`}</Tex>, a distance of typical size <Tex>{T`r_h`}</Tex> that grows under stress, and a
            share <Tex>{T`\alpha`}</Tex> of that distance carried from each quarter to the next. On this panel{" "}
            <Tex>{T`\alpha`}</Tex> is {fit.carry.toFixed(2)}. Lenders are also slow about news. When a change starts or ends below{" "}
            <Tex>{T`\bar v`}</Tex>, the report closes only part of the way to the new view:
          </p>
          <Tex block>{T`m_{h,q} = \varphi_h\, m_{h,q-1} + (1-\varphi_h)\,\tilde m_{h,q}`}</Tex>
          <p>
            Inverting that line recovers the view. Then a mark and the same lender's previous view, taken together, measure
          </p>
          <Tex block>{T`\tilde m_{h,q} - b_h - \alpha\,(\tilde m_{h,q-1} - b_h) \;=\; v_{a_q} - \alpha\, v_{a_{q-1}} + \varepsilon_{h,q}`}</Tex>
          <p>
            which is a linear reading of two values with only the fresh part of the disagreement as noise. A lender's own history
            anchors its marks without a single extra state per lender. Only the core of a distance fades this way, up to three
            of its own standard deviations. Whatever lies beyond is carried whole, because lenders ten points apart for two
            years are not arguing about one loan.
          </p>
          <p>
            A mark is a measurement of the past. The filter handles that by keeping the past in its state. On each reporting date
            it copies the current value into a slot, so the state becomes{" "}
            <Tex>{T`z_t = (v_t,\; c_t,\; v_{a_q},\; c_{a_q},\; v_{a_{q-1}},\; c_{a_{q-1}},\,\dots)`}</Tex>. The slots stay correlated with
            the present, and when a mark finally arrives, a reading <Tex>{T`y`}</Tex> of the combination{" "}
            <Tex>{T`\ell = v_{a_q} - \alpha v_{a_{q-1}}`}</Tex> corrects today's estimate by exactly the right amount:
          </p>
          <Tex block>{T`\hat v_t \leftarrow \hat v_t + \frac{\operatorname{Cov}(v_t, \ell)}{\operatorname{Var}(\ell) + (1-\alpha^2)\, r_h^2}\,\big(y - \hat \ell\big)`}</Tex>
          <p>
            This is why the band in every chart keeps widening past the end of a quarter and tightens only on the filing date.
          </p>

          <h3>What other loans say</h3>
          <p>
            The second slot component, <Tex>{T`c_a`}</Tex>, freezes the sector's cumulative shock over the quarter. Every loan is
            filtered alone, which is what makes thousands of them tractable, and then the link between them is restored. Each
            loan's first mark of a quarter surprises the public forecast by <Tex>{T`\nu_j = s + (\text{its own news and noise})`}</Tex>.
            The precision-weighted mean of the other loans' surprises, week by week as they are filed, is fed to each loan as an
            observation of <Tex>{T`c_a`}</Tex>, with its own surprise taken out so it never hears itself.
          </p>

          <h3>Jumps</h3>
          <p>
            Credit does not diffuse into default. A Gaussian filter reads a ten-point fall as a twenty-sigma valuation error and
            discounts it, which is the wrong inference. So the first mark of each reporting date is tested against a jump. With
            prior probability <Tex>{T`p`}</Tex> and jump variance <Tex>{T`\sigma_J^2`}</Tex>, a surprise <Tex>{T`\nu`}</Tex> whose
            variance without a jump would be <Tex>{T`S`}</Tex> gives the jump a posterior weight
          </p>
          <Tex block>{T`w = \frac{p\;\mathcal N(\nu;\,0,\,S+\sigma_J^2)}{p\;\mathcal N(\nu;\,0,\,S+\sigma_J^2) + (1-p)\;\mathcal N(\nu;\,0,\,S)}`}</Tex>
          <p>
            and the belief becomes the two-part mixture, each part updated on the mark, collapsed back to one Gaussian with the
            mixture's mean and covariance. Small surprises are filtered as noise; large ones are believed.
          </p>
        </div>
      </section>

      <section className="glass pane">
        <header>
          <h2>Estimating it</h2>
        </header>
        <div className="prose">
          <p>
            No parameter of the model is hand-set: each is identified by a different slice of the marks. What is fixed by design
            is a short list of thresholds, such as the price of 97 below which a loan counts as stressed, and the rules that
            clean the filings.
          </p>
          <ul>
            <li>
              <b>Public sensitivity, drift and pull to par.</b> Quarterly changes in the consensus mark are regressed on the
              change in the public spread. Every loan in a quarter sees the same move in spreads, so the estimate rests on as
              many independent observations as there are quarters. Each quarter is left out in turn and the sensitivity is
              shrunk by how far it moves. Here {fmtPct(fit.beta_kept[0], 0)} of it survives, and it explains{" "}
              {fmtPct(fit.r2_public, 1)} of the variation in marks.
            </li>
            <li>
              <b>Who is slow.</b> A fair value cannot be forecast from its own past. So if what the other lenders reported last
              quarter predicts a lender's change this quarter, that lender is marking late, by the ratio of two covariances:
              its change now against theirs then, over its change then against theirs then. No public market is needed. Pooled
              over {fmtInt(fit.n_lead_lag)} shared moves the share of an old mark carried into the next is{" "}
              {fit.phi_bar.toFixed(2)}: {fit.phi_stressed.toFixed(2)} where the lender already had the loan below 97, and{" "}
              {fit.phi_entering.toFixed(2)} where only the others did.
            </li>
            <li>
              <b>Bias and distance.</b> From how each lender's views sit against other lenders' on the same loan and date. A gap
              between one lender and the mean of the others mixes both sides' distance; a fixed-point iteration unpicks it.
            </li>
            <li>
              <b>How long a disagreement lasts.</b> A gap has a standing part and a wobble. Regressed on itself a quarter back
              its slope is {fit.carry_lags[0].toFixed(2)}, and two quarters back {fit.carry_lags[1].toFixed(2)}. The wobble
              dilutes both alike, so their ratio is the rate at which the standing part fades on its own.
            </li>
            <li>
              <b>Sector and loan volatility.</b> What public markets leave unexplained splits into a part shared by a sector in a
              quarter and a remainder, whose variance is fitted as a function of stress.
            </li>
            <li>
              <b>Jumps.</b> Frequency and size come from the tail of the same residuals.
            </li>
          </ul>
          <p>
            On this panel the fit gives a public sensitivity of {(d[0] * 100).toFixed(2)} points per point of spread for a loan at
            par, loan-specific volatility of {(d[2] * q * 100).toFixed(2)} points a quarter, sector volatility of{" "}
            {fit.sector_q_sd.toFixed(2)}, a typical distance between one lender and fair value of {fit.noise_median.toFixed(2)}{" "}
            points, and a jump in{" "}
            {fmtPct(d[12], 1)} of loan-quarters with a standard deviation of {(d[13] * 100).toFixed(0)} points. It used{" "}
            {fmtInt(fit.n_changes)} consecutive-quarter changes across {fit.n_quarters} quarters.
          </p>

          <h3>Making the error bar honest</h3>
          <p>
            The model's standard deviation says which forecasts are surer than others, but credit has a floor it does not see:
            the loan no one expected to move is the one that defaults. So an error is measured against the model's standard
            deviation plus the typical error of its regime, <Tex>{T`f`}</Tex>, separately for performing, watch-list and stressed
            loans. With scores <Tex>{T`e_i = |m_i - \hat m_i| / (\hat\sigma_i + f)`}</Tex> from <Tex>{T`n`}</Tex> earlier forecasts,
            the band at level <Tex>{T`1-\alpha`}</Tex> is
          </p>
          <Tex block>{T`\hat m \;\pm\; (\hat\sigma + f) \cdot e_{(k)}, \qquad k = \big\lceil (n+1)(1-\alpha) \big\rceil`}</Tex>
          <p>
            which holds its stated coverage whatever the distribution of errors, provided they are exchangeable. Across
            quarters they are not quite: when marks move more than in any quarter before, the band is too narrow for that
            quarter, and the evidence page shows by how much. The same floors and multipliers are applied to the band around
            the value itself.
          </p>
        </div>
      </section>

      <section className="glass pane">
        <header>
          <h2>From the error bar to a quote, and from a quote to a market</h2>
        </header>
        <div className="prose">
          <p>
            The half-spread is the sum of three charges. The first two are the closed-form quotes of Guéant, Lehalle and
            Fernandez-Tapia for a dealer answering requests that arrive at rate <Tex>{T`A e^{-k\delta}`}</Tex>, with risk aversion{" "}
            <Tex>{T`\gamma`}</Tex> and daily variance <Tex>{T`\sigma^2`}</Tex>:
          </p>
          <Tex block>{T`\delta = \underbrace{\frac{1}{\gamma}\ln\!\Big(1+\frac{\gamma}{k}\Big)}_{\text{making the market}} \;+\; \underbrace{\frac12\sqrt{\frac{\sigma^2\gamma}{2kA}\Big(1+\frac{\gamma}{k}\Big)^{1+k/\gamma}}}_{\text{carrying inventory}} \;+\; \underbrace{d(\pi)\,\hat\sigma_v}_{\text{being picked off}}`}</Tex>
          <p>
            The third is Glosten and Milgrom's. If a share <Tex>{T`\pi`}</Tex> of clients know the loan's value and trade only
            when the quote is wrong in their favour, a dealer quoting <Tex>{T`d`}</Tex> standard errors from its estimate breaks
            even where
          </p>
          <Tex block>{T`\frac{1-\pi}{2}\,d \;=\; \pi\,\big(\phi(d) - d\,\Phi(-d)\big)`}</Tex>
          <p>
            The charge is linear in the standard error <Tex>{T`\hat\sigma_v`}</Tex> of the mark. That is the link from statistics
            to money: halve the error bar and this part of the spread halves with it.
          </p>

          <h3>Information is liquidity</h3>
          <p>
            Let <Tex>{T`P`}</Tex> be the variance of the market's estimate. It grows at <Tex>{T`\sigma^2`}</Tex> a day and is cut
            to <Tex>{T`P r/(P+r)`}</Tex> by every observation with noise variance <Tex>{T`r`}</Tex>. Marks come on a schedule.
            Trades come at a rate that depends on the spread, which depends on <Tex>{T`P`}</Tex>. The desk view simulates that
            process exactly. Averaging over arrivals gives its skeleton,
          </p>
          <Tex block>{T`\frac{dP}{dt} = \sigma^2 \;-\; \frac{1}{T_m}\frac{P^2}{P+r_m} \;-\; 2A\,e^{-k\,\delta(P)}\,\frac{P^2}{P+r_t}`}</Tex>
          <p>
            whose last term is hump-shaped in <Tex>{T`P`}</Tex>: with little uncertainty there is little left to learn, with much
            the quote is too wide to trade on. Where the hump clears <Tex>{T`\sigma^2`}</Tex> there are three rest points, two of
            them stable. Setting the right side to zero and solving for <Tex>{T`A`}</Tex> gives the surface in the desk view as an
            explicit graph over <Tex>{T`P`}</Tex>, and a rest point is stable exactly where <Tex>{T`dA/dP < 0`}</Tex>.
          </p>
        </div>
      </section>

      <section className="glass pane">
        <header>
          <h2>What it does not do</h2>
        </header>
        <div className="prose">
          <ul>
            <li>
              It estimates where lenders would mark a loan if they marked it today without delay. That is a consensus of
              valuations, not a traded price. Where every lender is too generous, so is the estimate.
            </li>
            <li>
              Smoothing is read off serial dependence, as it is for hedge funds and private equity. A loan whose true value
              kept falling for reasons of its own would look the same as a lender marking it down late.
            </li>
            <li>
              One borrower and seniority can hide more than one loan. Pieces are split apart when their marks say so: a leap
              to par in a quarter, or two lenders ten points apart for a year. Where the marks do not say so, a first-out and a
              last-out are read as one loan, and their difference as disagreement.
            </li>
            <li>
              The desk's parameters, how many clients ask and how hard they shop, cannot be estimated from marks. They are
              assumptions to be varied, and every quote on this site inherits them.
            </li>
            <li>
              Borrowers are matched across lenders by name and loan terms. The rules prefer leaving two names apart to merging two
              companies, so some shared loans are missed.
            </li>
            <li>
              Between marks, the calibrated band holds the true value a little less often than stated in the simulated market,
              and on real filings the 90% band has held a little under 90% of outcomes. Both gaps are reported, not hidden.
            </li>
          </ul>
        </div>
      </section>

      <section className="glass pane">
        <header>
          <h2>How it is built</h2>
        </header>
        <div className="prose">
          <p>
            <b>Ingest, in Go.</b> Streams the SEC's Schedule of Investments reports straight out of their archives, parses each
            free-text identifier into borrower, seniority and facility, resolves borrowers across lenders, and writes a panel of
            marks stamped with the day each became public.
          </p>
          <p>
            <b>Kernel, in Rust, with no dependencies.</b> The filter, the estimation, the walk-forward backtest, the quote and
            the liquidity process. One source builds a native program for calibration and a WebAssembly module of about 150
            kilobytes. Every curve and quote on this site is computed in your browser by that module, from the exported marks, and
            matches the native program.
          </p>
          <p>
            <b>This page.</b> TypeScript, React and three.js, served as static files. There is no server.
          </p>
          <p>
            {synthetic ? (
              <>
                You are looking at a simulated market of {fmtInt(u.meta.counts.loans)} loans and {fmtInt(u.meta.counts.marks)}{" "}
                marks, written in the SEC's file format with a known answer, so that the whole pipeline can be tested and scored
                against the truth. No real company, lender or mark appears in it.
              </>
            ) : (
              <>
                You are looking at {fmtInt(u.meta.counts.loans)} loans and {fmtInt(u.meta.counts.marks)} marks from the SEC's BDC
                data sets, as filed. Names and marks are read by rule from free text, and some will be wrong.
              </>
            )}{" "}
            Public credit spreads are from FRED.
          </p>
        </div>
        <p className="footer" style={{ marginTop: 22 }}>
          Sources. SEC, Business Development Company data sets. Federal Reserve Bank of St. Louis, FRED. O. Guéant, C.-A. Lehalle and
          J. Fernandez-Tapia, Dealing with the inventory risk (2013). L. Glosten and P. Milgrom, Bid, ask and transaction prices in a
          specialist market with heterogeneously informed traders (1985). R. Huang and H. Stoll, The components of the bid-ask spread
          (1997). M. Getmansky, A. Lo and I. Makarov, An econometric model of serial correlation and illiquidity in hedge fund returns
          (2004). V. Vovk, A. Gammerman and G. Shafer, Algorithmic learning in a random world (2005).
        </p>
        <p className="footer">
          An independent research project, built from public filings. Lender names appear because they are in those filings; the
          figures beside them are statistical estimates, not findings about any manager's valuation process. Nothing here is
          investment advice.
        </p>
      </section>
    </div>
  );
}
