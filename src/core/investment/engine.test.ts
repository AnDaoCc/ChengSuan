import { describe, expect, it } from "vitest";
import {
  portfolio,
  evaluateAlerts,
  movingAverage,
  normalizeCandles,
} from "./engine";
import {
  defaultMonitorSettings,
  instrumentSchema,
  tradeSchema,
  type Trade,
  type InvestmentDocument,
  type MarketQuote,
} from "./model";
const accountId = "93f09c42-6a53-4c23-92b4-c24b264168aa";
const stock = instrumentSchema.parse({
  id: "cn:SH:600519",
  symbol: "600519",
  name: "测试股票",
  kind: "stock",
  market: "SH",
  unit: "股",
  currency: "CNY",
  provider: "cn",
  providerId: "1.600519",
});
function document(): InvestmentDocument {
  return {
    version: 1,
    revision: 0,
    accounts: [{ id: accountId, name: "测试", kind: "general", note: "" }],
    instruments: [stock],
    trades: [],
    watchlist: [],
    alerts: [],
    settings: defaultMonitorSettings,
  };
}
function trade(
  side: Trade["side"],
  quantity: string,
  unitPrice: string,
  sequence = 0,
): Trade {
  return {
    id: crypto.randomUUID(),
    accountId,
    instrumentId: stock.id,
    side,
    quantity,
    unitPrice,
    fee: "0",
    currency: "CNY",
    fxToCny: "1",
    fxDate: "2026-10-13",
    fxSource: "手动",
    executedAt: "2026-10-13T00:00:00Z",
    sequence,
    note: "",
  };
}
function quote(
  price = "33",
  status: MarketQuote["status"] = "live",
): MarketQuote {
  return {
    instrumentId: stock.id,
    price,
    currency: "CNY",
    cnyRate: "1",
    fxDate: "2026-10-13",
    changePercent: "1",
    source: "测试",
    sourceAt: "2026-10-13T00:01:00Z",
    fetchedAt: "2026-10-13T00:01:00Z",
    status,
    note: "",
  };
}
describe("investment accounting", () => {
  it("replays weighted costs, partial sales and fees exactly", () => {
    const d = document();
    d.trades = [
      { ...trade("buy", "10", "10"), fee: "2" },
      trade("buy", "10", "20", 1),
      { ...trade("sell", "5", "30", 2), fee: "1" },
    ];
    const r = portfolio(d, [quote()]);
    expect(r.positions[0]).toMatchObject({
      quantity: "15",
      costCny: "226.5",
      averageCostCny: "15.1",
      realizedCny: "73.5",
      valueCny: "495",
      gainCny: "268.5",
    });
  });
  it("clears residual costs on full sale and rejects historical overselling", () => {
    const d = document();
    d.trades = [trade("buy", "3", "10"), trade("sell", "3", "12", 1)];
    expect(portfolio(d).positions[0]).toMatchObject({
      quantity: "0",
      costCny: "0",
      realizedCny: "6",
      valueCny: "0",
    });
    d.trades[0].quantity = "2";
    expect(() => portfolio(d)).toThrow("超过");
  });
  it("freezes trade FX and uses actual USDT conversion rather than a fixed peg", () => {
    const d = document();
    d.instruments[0] = {
      ...stock,
      kind: "crypto",
      market: "CRYPTO",
      unit: "币",
      provider: "coingecko",
      providerId: "bitcoin",
      currency: "USD",
    };
    d.trades = [
      {
        ...trade("buy", "0.01", "20000"),
        currency: "USDT",
        fxToCny: "6.9",
        fee: "1",
      },
    ];
    const r = portfolio(d, [
      { ...quote("21000"), currency: "USD", cnyRate: "6.8" },
    ]);
    expect(r.costCny).toBe("1386.9");
    expect(r.knownValueCny).toBe("1428");
    expect(r.knownGainCny).toBe("41.1");
  });
  it("keeps small token quantities and prices precise", () => {
    const d = document();
    d.instruments[0] = { ...stock, kind: "crypto" };
    d.trades = [trade("buy", "0.000000000123456789", "0.000000000123456789")];
    expect(portfolio(d, [quote("0.000000000123456789")]).knownGainCny).toBe(
      "0",
    );
  });
  it("applies gold purity and marks manual estimates", () => {
    const d = document();
    d.instruments[0] = {
      ...stock,
      kind: "gold",
      market: "SGE",
      unit: "克",
      purity: "0.999",
      manualPrice: "900",
    };
    d.trades = [trade("buy", "2.5", "800")];
    const r = portfolio(d);
    expect(r.positions[0].status).toBe("manual");
    expect(r.complete).toBe(true);
    expect(r.knownValueCny.startsWith("2247.974797")).toBe(true);
  });
  it("keeps absent quotes null and cached values explicitly estimated", () => {
    const d = document();
    d.trades = [trade("buy", "1", "10")];
    expect(portfolio(d)).toMatchObject({ complete: false, knownValueCny: "0" });
    expect(portfolio(d).positions[0].valueCny).toBeNull();
    expect(portfolio(d, [quote("12", "cached")])).toMatchObject({
      complete: true,
      estimated: true,
    });
  });
  it("separates accounts and supports corrections with preserved realized gains", () => {
    const d = document();
    const other = crypto.randomUUID();
    d.accounts.push({
      id: other,
      name: "另一个账户",
      kind: "wallet",
      note: "",
    });
    d.trades = [
      trade("buy", "10", "10"),
      { ...trade("buy", "20", "20"), accountId: other },
      { ...trade("adjust", "5", "30", 1), note: "送股后核对实际数量与成本" },
    ];
    expect(portfolio(d, [], accountId).costCny).toBe("150");
    expect(portfolio(d).costCny).toBe("550");
  });
  it("validates integer shares, duplicate trades and opening order", () => {
    const d = document();
    d.trades = [trade("buy", "0.5", "10")];
    expect(() => portfolio(d)).toThrow("整数");
    d.trades = [trade("buy", "1", "10"), trade("opening", "1", "10", 1)];
    expect(() => portfolio(d)).toThrow("起始");
    d.trades = [trade("buy", "1", "10")];
    d.trades.push(d.trades[0]);
    expect(() => portfolio(d)).toThrow("重复");
  });
});
it("alerts latch once, rearm on crossing and obey cooldown and quote quality", () => {
  const d = document();
  d.alerts = [
    {
      id: crypto.randomUUID(),
      instrumentId: stock.id,
      accountId: null,
      kind: "priceAbove",
      threshold: "20",
      enabled: true,
      cooldownMinutes: 15,
    },
  ];
  const now = Date.parse("2026-10-13T00:01:00Z");
  const first = evaluateAlerts(d, [quote("21")], {}, now);
  expect(first.events).toHaveLength(1);
  expect(
    evaluateAlerts(
      d,
      [{ ...quote("22"), sourceAt: new Date(now + 1000).toISOString() }],
      first.states,
      now + 1000,
    ).events,
  ).toHaveLength(0);
  const inside = evaluateAlerts(
    d,
    [{ ...quote("19"), sourceAt: new Date(now + 1000).toISOString() }],
    first.states,
    now + 1000,
  );
  expect(
    evaluateAlerts(
      d,
      [{ ...quote("22"), sourceAt: new Date(now + 2000).toISOString() }],
      inside.states,
      now + 2000,
    ).events,
  ).toHaveLength(0);
  const later = now + 16 * 60000;
  expect(
    evaluateAlerts(
      d,
      [
        {
          ...quote("22"),
          sourceAt: new Date(later).toISOString(),
          fetchedAt: new Date(later).toISOString(),
        },
      ],
      inside.states,
      later,
    ).events,
  ).toHaveLength(1);
  expect(
    evaluateAlerts(d, [quote("30", "cached")], {}, now).events,
  ).toHaveLength(0);
});
it("sorts and deduplicates candles and computes decimal moving averages", () => {
  const candles = [3, 1, 2, 4, 5].map((n) => ({
    time: n,
    open: String(n),
    high: String(n),
    low: String(n),
    close: String(n),
    volume: null,
  }));
  expect(normalizeCandles([...candles, candles[0]])).toHaveLength(5);
  expect(movingAverage(candles, 5)).toEqual([{ time: 5, value: "3" }]);
  expect(() => normalizeCandles([{ ...candles[0], high: "1" }])).toThrow();
});
it("invalid decimal inputs return validation errors without throwing decimal parser exceptions", () => {
  for (const input of ["abc", "-1", "1e10", "Infinity", "", "0."])
    expect(
      tradeSchema.safeParse({ ...trade("buy", "1", "1"), quantity: input })
        .success,
    ).toBe(false);
  expect(instrumentSchema.safeParse({ ...stock, purity: "bad" }).success).toBe(
    false,
  );
  expect(() => movingAverage([], 0)).toThrow("正整数");
});
it("a replayed quote cannot re-trigger a rule after cooldown without a new source timestamp", () => {
  const d = document();
  const id = crypto.randomUUID();
  d.alerts = [
    {
      id,
      instrumentId: stock.id,
      accountId: null,
      kind: "priceAbove",
      threshold: "20",
      enabled: true,
      cooldownMinutes: 15,
    },
  ];
  const later = Date.parse(quote().sourceAt) + 16 * 60000;
  expect(
    evaluateAlerts(
      d,
      [{ ...quote("30"), fetchedAt: new Date(later).toISOString() }],
      {
        [id]: {
          latched: false,
          lastTriggeredAt: later - 16 * 60000,
          lastQuoteAt: quote().sourceAt,
        },
      },
      later,
    ).events,
  ).toHaveLength(0);
});
