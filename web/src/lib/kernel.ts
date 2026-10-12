// The kernel, compiled to WebAssembly.
//
// Its interface is plain C functions over linear memory: allocate a buffer of
// doubles, write, call, read. This module hides the pointers and nothing else.

interface Exports {
  memory: WebAssembly.Memory;
  px_alloc(len: number): number;
  px_free(ptr: number, len: number): void;
  px_curve(
    dynamics: number,
    shocks: number,
    nDays: number,
    quarters: number,
    nQuarters: number,
    marks: number,
    nMarks: number,
    sector: number,
    nSector: number,
    prints: number,
    nPrints: number,
    start: number,
    end: number,
    out: number,
    forecasts: number,
  ): number;
  px_daily_var(dynamics: number, v: number, daysPerQuarter: number): number;
  px_quote(desk: number, mid: number, sd: number, dailyVar: number, out: number): void;
  px_adverse_width(informed: number): number;
  px_rest(market: number, out: number, max: number): number;
  px_manifold(market: number, sd: number, n: number, out: number): void;
  px_simulate(market: number, sd0: number, days: number, seed: number, out: number): void;
  px_census(market: number, years: number, seed: number, out: number): void;
}

export interface Desk {
  /** Requests for quotes per day, each side, that a zero spread would win. */
  arrival: number;
  /** Fall-off of the win rate per point of half-spread. */
  decay: number;
  /** Risk aversion per point of profit and loss. */
  gamma: number;
  /** Share of clients who know the value. */
  informed: number;
  /** Position in clips; positive is long. */
  inventory: number;
}

export interface Market {
  desk: Desk;
  dailyVar: number;
  markGap: number;
  markSd: number;
  printSd: number;
  subsidy: number;
  subsidyAbove: number;
}

export interface Quote {
  mid: number;
  bid: number;
  ask: number;
  halfSpread: number;
  processing: number;
  inventoryRisk: number;
  adverseSelection: number;
  skew: number;
  bidRate: number;
  askRate: number;
}

export interface RestPoint {
  sd: number;
  spread: number;
  rate: number;
  stable: boolean;
}

export interface Step {
  sd: number;
  spread: number;
  rate: number;
  trades: number;
  subsidised: number;
  marked: boolean;
}

export interface Census {
  tradeable: number;
  meanSpread: number;
  medianSpread: number;
  tradesPerYear: number;
  subsidyPerYear: number;
  sdMax: number;
  hist: number[];
}

export interface Curve {
  /** Real-time estimate and its standard deviation, as fractions of par. */
  mean: Float64Array;
  sd: Float64Array;
  /** The same with every mark ever filed in hand. */
  hindMean: Float64Array;
  hindSd: Float64Array;
  /** Six per mark: public mean, sd, syndicate mean, sd, marks already seen, late flag. */
  forecasts: Float64Array;
}

/**
 * Floats per mark, as the kernel reads them: year-month, reporting day,
 * filing day, price, the lender's previous price, what that previous mark
 * implied about fair value, then the lender's smoothing, bias, distance and
 * carry, and its number.
 */
export const MARK_LEN = 11;

export interface CurveInput {
  dynamics: number[];
  shocks: Float64Array;
  quarters: ArrayLike<number>;
  /** Eight per mark. */
  marks: ArrayLike<number>;
  /** Four per entry. */
  sector: ArrayLike<number>;
  /** Three per print. */
  prints: ArrayLike<number>;
  start: number;
  end: number;
}

const CENSUS_BINS = 40;

function deskRow(d: Desk): number[] {
  return [d.arrival, d.decay, d.gamma, d.informed, d.inventory];
}

function marketRow(m: Market): number[] {
  return [...deskRow(m.desk), m.dailyVar, m.markGap, m.markSd, m.printSd, m.subsidy, m.subsidyAbove];
}

export class Kernel {
  private constructor(private x: Exports) {}

  static async load(url = "./syndicate_kernel.wasm"): Promise<Kernel> {
    const res = await fetch(url);
    if (!res.ok) {
      throw new Error(`${url}: ${res.status} ${res.statusText}`);
    }
    const { instance } = await WebAssembly.instantiate(await res.arrayBuffer(), {});
    return new Kernel(instance.exports as unknown as Exports);
  }

  /** Same, from bytes already in hand. Used outside the browser. */
  static async fromBytes(bytes: BufferSource): Promise<Kernel> {
    const { instance } = await WebAssembly.instantiate(bytes, {});
    return new Kernel(instance.exports as unknown as Exports);
  }

  // A view must be taken after every allocation: growing the memory detaches
  // earlier ones.
  private view(ptr: number, len: number): Float64Array {
    return new Float64Array(this.x.memory.buffer, ptr, len);
  }

  private put(values: ArrayLike<number>): [number, number] {
    const n = values.length;
    const ptr = this.x.px_alloc(n);
    this.view(ptr, n).set(values);
    return [ptr, n];
  }

  private with<T>(inputs: ArrayLike<number>[], outLens: number[], call: (ins: number[], outs: number[]) => void, read: (outs: Float64Array[]) => T): T {
    const ins = inputs.map((v) => this.put(v));
    const outs = outLens.map((n) => [this.x.px_alloc(n), n] as [number, number]);
    try {
      call(
        ins.map((p) => p[0]),
        outs.map((p) => p[0]),
      );
      return read(outs.map(([ptr, n]) => this.view(ptr, n).slice()));
    } finally {
      for (const [ptr, n] of [...ins, ...outs]) {
        this.x.px_free(ptr, n);
      }
    }
  }

  curve(input: CurveInput): Curve {
    const n = input.end - input.start + 1;
    const nMarks = input.marks.length / MARK_LEN;
    return this.with(
      [input.dynamics, input.shocks, input.quarters, input.marks, input.sector, input.prints],
      [4 * n, Math.max(1, 6 * nMarks)],
      ([dyn, shocks, quarters, marks, sector, prints], [out, fc]) => {
        const code = this.x.px_curve(
          dyn,
          shocks,
          input.shocks.length,
          quarters,
          input.quarters.length / 2,
          marks,
          nMarks,
          sector,
          input.sector.length / 4,
          prints,
          input.prints.length / 3,
          input.start,
          input.end,
          out,
          fc,
        );
        if (code !== 0) {
          throw new Error(`kernel rejected the run (code ${code})`);
        }
      },
      ([out, fc]) => ({
        mean: out.subarray(0, n),
        sd: out.subarray(n, 2 * n),
        hindMean: out.subarray(2 * n, 3 * n),
        hindSd: out.subarray(3 * n, 4 * n),
        forecasts: fc.subarray(0, 6 * nMarks),
      }),
    );
  }

  /** Daily variance of value at price `v` (a fraction of par), jumps included. */
  dailyVar(dynamics: number[], v: number, daysPerQuarter: number): number {
    const [ptr, n] = this.put(dynamics);
    try {
      return this.x.px_daily_var(ptr, v, daysPerQuarter);
    } finally {
      this.x.px_free(ptr, n);
    }
  }

  quote(desk: Desk, mid: number, sd: number, dailyVar: number): Quote {
    return this.with(
      [deskRow(desk)],
      [10],
      ([d], [out]) => this.x.px_quote(d, mid, sd, dailyVar, out),
      ([o]) => ({
        mid: o[0],
        bid: o[1],
        ask: o[2],
        halfSpread: o[3],
        processing: o[4],
        inventoryRisk: o[5],
        adverseSelection: o[6],
        skew: o[7],
        bidRate: o[8],
        askRate: o[9],
      }),
    );
  }

  adverseWidth(informed: number): number {
    return this.x.px_adverse_width(informed);
  }

  rest(market: Market): RestPoint[] {
    const max = 8;
    let count = 0;
    return this.with(
      [marketRow(market)],
      [4 * max],
      ([m], [out]) => {
        count = this.x.px_rest(m, out, max);
      },
      ([o]) => {
        const rows: RestPoint[] = [];
        for (let i = 0; i < count; i++) {
          rows.push({ sd: o[4 * i], spread: o[4 * i + 1], rate: o[4 * i + 2], stable: o[4 * i + 3] === 1 });
        }
        return rows;
      },
    );
  }

  /** For each standard error, the arrival rate at which it is a rest point. */
  manifold(market: Market, sds: ArrayLike<number>): Float64Array {
    return this.with(
      [marketRow(market), sds],
      [sds.length],
      ([m, s], [out]) => this.x.px_manifold(m, s, sds.length, out),
      ([o]) => o,
    );
  }

  simulate(market: Market, sd0: number, days: number, seed: number): Step[] {
    return this.with(
      [marketRow(market)],
      [6 * days],
      ([m], [out]) => this.x.px_simulate(m, sd0, days, seed, out),
      ([o]) => {
        const steps: Step[] = new Array(days);
        for (let i = 0; i < days; i++) {
          steps[i] = {
            sd: o[6 * i],
            spread: o[6 * i + 1],
            rate: o[6 * i + 2],
            trades: o[6 * i + 3],
            subsidised: o[6 * i + 4],
            marked: o[6 * i + 5] === 1,
          };
        }
        return steps;
      },
    );
  }

  census(market: Market, years: number, seed: number): Census {
    return this.with(
      [marketRow(market)],
      [6 + CENSUS_BINS],
      ([m], [out]) => this.x.px_census(m, years, seed, out),
      ([o]) => ({
        tradeable: o[0],
        meanSpread: o[1],
        medianSpread: o[2],
        tradesPerYear: o[3],
        subsidyPerYear: o[4],
        sdMax: o[5],
        hist: Array.from(o.subarray(6)),
      }),
    );
  }
}
