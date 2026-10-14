import Decimal from "decimal.js";
import {
  documentSchema,
  candleSchema,
  type InvestmentDocument,
  type MarketQuote,
  type PortfolioResult,
  type PortfolioPosition,
  type Candle,
  type AlertState,
  type AlertEvent,
} from "./model";

export const D = Decimal.clone({
  precision: 512,
  rounding: Decimal.ROUND_HALF_UP,
});
export function portfolio(
  input: InvestmentDocument,
  quotes: readonly MarketQuote[] = [],
  accountId?: string,
): PortfolioResult {
  const doc = documentSchema.parse(input);
  const assets = new Map(doc.instruments.map((i) => [i.id, i]));
  const accounts = new Set(doc.accounts.map((a) => a.id));
  if (new Set(doc.trades.map((t) => t.id)).size !== doc.trades.length)
    throw Error("交易记录 ID 重复");
  if (
    assets.size !== doc.instruments.length ||
    accounts.size !== doc.accounts.length
  )
    throw Error("账户或标的 ID 重复");
  const balances = new Map<
    string,
    {
      accountId: string;
      instrumentId: string;
      quantity: Decimal;
      cost: Decimal;
      realized: Decimal;
      started: boolean;
    }
  >();
  const trades = [...doc.trades].sort(
    (a, b) =>
      Date.parse(a.executedAt) - Date.parse(b.executedAt) ||
      a.sequence - b.sequence ||
      a.id.localeCompare(b.id),
  );
  for (const t of trades) {
    const asset = assets.get(t.instrumentId);
    if (!asset || !accounts.has(t.accountId))
      throw Error("交易记录引用的账户或标的不存在");
    if (asset.kind === "index")
      throw Error("指数用于行情参考，请选择可交易的股票或 ETF");
    if (
      (asset.kind === "stock" || asset.kind === "etf") &&
      !new D(t.quantity).isInteger()
    )
      throw Error("股票和 ETF 的股／份数量必须是整数");
    const key = `${t.accountId}|${t.instrumentId}`;
    const p = balances.get(key) ?? {
      accountId: t.accountId,
      instrumentId: t.instrumentId,
      quantity: new D(0),
      cost: new D(0),
      realized: new D(0),
      started: false,
    };
    const qty = new D(t.quantity),
      gross = qty.mul(t.unitPrice).mul(t.fxToCny),
      fee = new D(t.fee).mul(t.fxToCny);
    if (t.side === "opening" && p.started)
      throw Error("起始持仓必须排在该账户标的的其他交易之前");
    if (t.side === "buy" || t.side === "opening") {
      p.quantity = p.quantity.plus(qty);
      p.cost = p.cost.plus(gross).plus(fee);
    }
    if (t.side === "sell") {
      if (qty.gt(p.quantity))
        throw Error(
          `${asset.name} 在 ${t.executedAt.slice(0, 10)} 卖出数量超过当时持仓`,
        );
      const released = qty.eq(p.quantity)
        ? p.cost
        : p.cost.mul(qty).div(p.quantity);
      p.realized = p.realized.plus(gross.minus(fee).minus(released));
      p.cost = p.cost.minus(released);
      p.quantity = p.quantity.minus(qty);
    }
    if (t.side === "adjust") {
      p.quantity = qty;
      p.cost = qty.mul(t.unitPrice);
    }
    p.started = true;
    balances.set(key, p);
  }
  const positions: PortfolioPosition[] = [];
  for (const p of balances.values()) {
    if (accountId && accountId !== p.accountId) continue;
    const asset = assets.get(p.instrumentId)!;
    const quote = quotes.find((q) => q.instrumentId === p.instrumentId);
    let price: Decimal | null = null;
    let status: PortfolioPosition["status"] = quote?.status ?? "missing";
    if (asset.manualPrice) {
      price = new D(asset.manualPrice);
      status = "manual";
    } else if (quote && (quote.currency === "CNY" || quote.cnyRate))
      price = new D(quote.price).mul(
        quote.currency === "CNY" ? "1" : quote.cnyRate!,
      );
    if (price && asset.kind === "gold")
      price = price.mul(asset.purity).div("0.9999");
    const value = p.quantity.isZero()
      ? new D(0)
      : (price?.mul(p.quantity) ?? null);
    const gain = value?.minus(p.cost) ?? null;
    positions.push({
      accountId: p.accountId,
      instrumentId: p.instrumentId,
      quantity: p.quantity.toFixed(),
      costCny: p.cost.toFixed(),
      averageCostCny: p.quantity.isZero()
        ? "0"
        : p.cost.div(p.quantity).toFixed(),
      realizedCny: p.realized.toFixed(),
      valueCny: value?.toFixed() ?? null,
      gainCny: gain?.toFixed() ?? null,
      gainPercent:
        gain && !p.cost.isZero() ? gain.div(p.cost).mul(100).toFixed() : null,
      priceCny: price?.toFixed() ?? null,
      status,
    });
  }
  const sum = (values: (string | null)[]) =>
    values.reduce<Decimal>((a, v) => a.plus(v ?? "0"), new D(0)).toFixed();
  const held = positions.filter((p) => new D(p.quantity).gt(0));
  return {
    positions,
    costCny: sum(held.map((p) => p.costCny)),
    knownValueCny: sum(held.map((p) => p.valueCny)),
    realizedCny: sum(positions.map((p) => p.realizedCny)),
    knownGainCny: sum(held.map((p) => p.gainCny)),
    complete: held.every((p) => p.valueCny !== null),
    estimated: held.some((p) => p.status !== "live"),
  };
}

export function evaluateAlerts(
  doc: InvestmentDocument,
  quotes: readonly MarketQuote[],
  previous: Record<string, AlertState>,
  now = Date.now(),
) {
  const result = portfolio(doc, quotes);
  const states = { ...previous };
  const events: AlertEvent[] = [];
  for (const rule of doc.alerts) {
    if (!rule.enabled) continue;
    const quote = quotes.find((q) => q.instrumentId === rule.instrumentId);
    const instrument = doc.instruments.find((i) => i.id === rule.instrumentId);
    if (
      !quote ||
      quote.status !== "live" ||
      !instrument ||
      instrument.manualPrice ||
      now - Date.parse(quote.fetchedAt) > 180000
    )
      continue;
    const existing = previous[rule.id] ?? {
      latched: false,
      lastTriggeredAt: 0,
      lastQuoteAt: "",
    };
    if (!quote.sourceAt || quote.sourceAt === existing.lastQuoteAt) continue;
    const positions = result.positions.filter(
      (p) =>
        p.instrumentId === rule.instrumentId &&
        (!rule.accountId || p.accountId === rule.accountId),
    );
    let value: Decimal | null = null;
    if (rule.kind.startsWith("price")) {
      if (quote.currency === "CNY" || quote.cnyRate)
        value = new D(quote.price).mul(
          quote.currency === "CNY" ? "1" : quote.cnyRate!,
        );
      if (value && instrument.kind === "gold")
        value = value.mul(instrument.purity).div("0.9999");
    } else if (positions.length && positions.every((p) => p.gainCny !== null)) {
      const gain = positions.reduce((s, p) => s.plus(p.gainCny!), new D(0));
      const cost = positions.reduce((s, p) => s.plus(p.costCny), new D(0));
      value = rule.kind.startsWith("percent")
        ? cost.isZero()
          ? null
          : gain.div(cost).mul(100)
        : gain;
    }
    if (!value) continue;
    const matches = rule.kind.endsWith("Above")
      ? value.gte(rule.threshold)
      : value.lte(rule.threshold);
    const state = { ...existing, lastQuoteAt: quote.sourceAt };
    if (!matches) state.latched = false;
    else if (
      !state.latched &&
      (state.lastTriggeredAt === 0 ||
        now - state.lastTriggeredAt >= rule.cooldownMinutes * 60000)
    ) {
      state.latched = true;
      state.lastTriggeredAt = now;
      events.push({
        id: crypto.randomUUID(),
        ruleId: rule.id,
        instrumentId: rule.instrumentId,
        triggeredAt: new Date(now).toISOString(),
        message: `${instrument.name} ${rule.kind.startsWith("price") ? "人民币估价" : rule.kind.startsWith("percent") ? "持仓盈亏比例" : "持仓盈亏"}达到 ${value.toSignificantDigits(10).toFixed()}${rule.kind.startsWith("percent") ? "%" : " 元"}，阈值 ${rule.threshold}`,
      });
    }
    states[rule.id] = state;
  }
  return { states, events };
}

export function normalizeCandles(input: readonly Candle[]): Candle[] {
  const unique = new Map<number, Candle>();
  for (const c of input) {
    candleSchema.parse(c);
    if (
      new D(c.low).gt(c.high) ||
      new D(c.high).lt(D.max(c.open, c.close)) ||
      new D(c.low).gt(D.min(c.open, c.close))
    )
      throw Error("K 线开高低收关系无效");
    unique.set(c.time, c);
  }
  return [...unique.values()].sort((a, b) => a.time - b.time);
}
export function movingAverage(
  input: readonly Candle[],
  period: number,
): { time: number; value: string }[] {
  if (!Number.isInteger(period) || period < 1)
    throw Error("均线周期必须是正整数");
  const bars = normalizeCandles(input);
  const result: { time: number; value: string }[] = [];
  let sum = new D(0);
  for (let i = 0; i < bars.length; i++) {
    sum = sum.plus(bars[i].close);
    if (i >= period) sum = sum.minus(bars[i - period].close);
    if (i >= period - 1)
      result.push({ time: bars[i].time, value: sum.div(period).toFixed() });
  }
  return result;
}
