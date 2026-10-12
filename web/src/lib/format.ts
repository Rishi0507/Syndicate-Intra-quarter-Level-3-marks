const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** 20240331 -> "31 Mar 2024" */
export function fmtDay(d: number): string {
  return `${d % 100} ${MONTHS[(Math.floor(d / 100) % 100) - 1]} ${Math.floor(d / 10000)}`;
}

/** 20240331 -> "Mar 2024" */
export function fmtMonth(d: number): string {
  return `${MONTHS[(Math.floor(d / 100) % 100) - 1]} ${Math.floor(d / 10000)}`;
}

/** Year-month index -> "Q1 2024" */
export function fmtQuarter(ym: number): string {
  return `Q${Math.floor((ym % 12) / 3) + 1} ${Math.floor(ym / 12)}`;
}

export function fmtPrice(points: number, digits = 2): string {
  return Number.isFinite(points) ? points.toFixed(digits) : "–";
}

export function fmtSigned(x: number, digits = 2): string {
  if (!Number.isFinite(x)) {
    return "–";
  }
  const s = Math.abs(x).toFixed(digits);
  return x > 0 ? `+${s}` : x < 0 ? `−${s}` : s;
}

export function fmtPct(share: number, digits = 0): string {
  return Number.isFinite(share) ? `${(share * 100).toFixed(digits)}%` : "–";
}

export function fmtInt(n: number): string {
  return Math.round(n).toLocaleString("en-US");
}

/** $ millions -> "$12.4m", "$1.2bn" */
export function fmtSize(millions: number | null): string {
  if (millions == null || !Number.isFinite(millions)) {
    return "–";
  }
  if (millions >= 1000) {
    return `$${(millions / 1000).toFixed(1)}bn`;
  }
  return `$${millions >= 100 ? millions.toFixed(0) : millions.toFixed(1)}m`;
}

const SECTOR_LABELS: Record<string, string> = {
  software: "Software and technology",
  healthcare: "Healthcare",
  business: "Business services",
  financial: "Financials and insurance",
  consumer: "Consumer",
  industrial: "Industrials",
  aerospace: "Aerospace and defense",
  energy: "Energy and utilities",
  media: "Media and telecom",
  transport: "Transportation and logistics",
  realestate: "Real estate and construction",
  education: "Education",
  food: "Food and beverage",
  materials: "Chemicals and materials",
  other: "Other",
};

export function sectorLabel(code: string): string {
  return SECTOR_LABELS[code] ?? code;
}

export function classLabel(code: string): string {
  return code === "senior" ? "Senior debt" : code === "junior" ? "Junior debt" : "Debt, seniority not stated";
}
