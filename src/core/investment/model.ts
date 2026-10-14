import { z } from "zod";
import Decimal from "decimal.js";

export const decimalText = z
  .string()
  .max(100)
  .regex(/^\d+(\.\d+)?$/, "请输入非负十进制数");
export const signedText = z
  .string()
  .max(120)
  .regex(/^-?\d+(\.\d+)?$/, "请输入十进制数");
const isDecimal = (v: string) => /^\d+(\.\d+)?$/.test(v) && v.length <= 100;
export const positiveText = decimalText.refine(
  (v) => isDecimal(v) && new Decimal(v).gt(0),
  "必须大于 0",
);
export const quoteCurrencySchema = z.enum(["CNY", "USD", "USDT"]);
export type QuoteCurrency = z.infer<typeof quoteCurrencySchema>;
export const poolSchema = z.object({
  address: z.string().min(1).max(250),
  name: z.string().max(400),
  liquidity: decimalText,
  tokenSide: z.enum(["base", "quote"]),
});
export const instrumentSchema = z.object({
  id: z.string().min(1).max(300),
  symbol: z.string().min(1).max(100),
  name: z.string().min(1).max(400),
  kind: z.enum(["stock", "etf", "index", "gold", "crypto"]),
  market: z.enum(["SH", "SZ", "BJ", "SGE", "CRYPTO"]),
  unit: z.string().max(20),
  currency: quoteCurrencySchema,
  provider: z.enum(["cn", "sge", "coingecko", "gecko"]),
  providerId: z.string().min(1).max(300),
  goldEtf: z.boolean().default(false),
  chain: z.string().max(100).optional(),
  contract: z.string().max(250).optional(),
  decimals: z.number().int().min(0).max(255).optional(),
  pools: z.array(poolSchema).max(100).default([]),
  pool: poolSchema.optional(),
  purity: positiveText
    .refine((v) => isDecimal(v) && new Decimal(v).lte(1), "纯度不能超过 1")
    .default("0.9999"),
  manualPrice: positiveText.nullable().default(null),
});
export type Instrument = z.infer<typeof instrumentSchema>;
export const accountSchema = z.object({
  id: z.string().uuid(),
  name: z.string().trim().min(1).max(100),
  kind: z.enum(["general", "securities", "exchange", "wallet", "gold"]),
  note: z.string().max(1000),
});
export type InvestmentAccount = z.infer<typeof accountSchema>;
export const tradeSchema = z
  .object({
    id: z.string().uuid(),
    accountId: z.string().uuid(),
    instrumentId: z.string().min(1).max(300),
    side: z.enum(["buy", "sell", "opening", "adjust"]),
    executedAt: z.string().datetime(),
    sequence: z.number().int().min(0),
    quantity: decimalText,
    unitPrice: decimalText,
    fee: decimalText,
    currency: quoteCurrencySchema,
    fxToCny: positiveText,
    fxDate: z.string().regex(/^\d{4}-\d{2}-\d{2}$/),
    fxSource: z.string().min(1).max(200),
    note: z.string().max(4000),
  })
  .superRefine((t, ctx) => {
    if (![t.quantity, t.unitPrice, t.fee, t.fxToCny].every(isDecimal)) return;
    if (t.side !== "adjust" && !new Decimal(t.quantity).gt(0))
      ctx.addIssue({
        code: "custom",
        message: "成交数量必须大于 0",
        path: ["quantity"],
      });
    if (
      (t.side === "buy" || t.side === "sell") &&
      !new Decimal(t.unitPrice).gt(0)
    )
      ctx.addIssue({
        code: "custom",
        message: "成交价格必须大于 0",
        path: ["unitPrice"],
      });
    if (t.currency === "CNY" && !new Decimal(t.fxToCny).eq(1))
      ctx.addIssue({
        code: "custom",
        message: "人民币换算率必须为 1",
        path: ["fxToCny"],
      });
    if (
      t.side === "adjust" &&
      (t.currency !== "CNY" || !new Decimal(t.fee).isZero() || !t.note.trim())
    )
      ctx.addIssue({
        code: "custom",
        message: "持仓校正必须用人民币成本、零费用，并填写依据",
      });
  });
export type Trade = z.infer<typeof tradeSchema>;
export const quoteSchema = z.object({
  instrumentId: z.string(),
  price: positiveText,
  currency: quoteCurrencySchema,
  cnyRate: positiveText.nullable(),
  fxDate: z.string(),
  changePercent: signedText.nullable(),
  source: z.string().max(200),
  sourceAt: z.string(),
  fetchedAt: z.string().datetime(),
  status: z.enum(["live", "closed", "cached", "manual", "delayed"]),
  note: z.string().max(1000),
});
export type MarketQuote = z.infer<typeof quoteSchema>;
export const candleSchema = z.object({
  time: z.number().int(),
  open: positiveText,
  high: positiveText,
  low: positiveText,
  close: positiveText,
  volume: decimalText.nullable(),
});
export type Candle = z.infer<typeof candleSchema>;
export const alertSchema = z.object({
  id: z.string().uuid(),
  instrumentId: z.string(),
  accountId: z.string().uuid().nullable(),
  kind: z.enum([
    "priceAbove",
    "priceBelow",
    "gainAbove",
    "gainBelow",
    "percentAbove",
    "percentBelow",
  ]),
  threshold: signedText,
  enabled: z.boolean(),
  cooldownMinutes: z.number().int().min(1).max(1440),
});
export type AlertRule = z.infer<typeof alertSchema>;
export type AlertState = {
  latched: boolean;
  lastTriggeredAt: number;
  lastQuoteAt: string;
};
export type AlertEvent = {
  id: string;
  ruleId: string;
  instrumentId: string;
  message: string;
  triggeredAt: string;
};
export const monitorSettingsSchema = z.object({
  stockSeconds: z.number().int().min(15).max(3600),
  cryptoSeconds: z.number().int().min(30).max(3600),
  goldSeconds: z.number().int().min(30).max(3600),
  coinGeckoMode: z.enum(["public", "demo", "pro"]),
  allTickEnabled: z.boolean(),
  allTickTransport: z.enum(["polling", "push"]).default("polling"),
  allTickSubscriptionCap: z.number().int().min(1).max(3000).default(5),
  allTickDailyCap: z.number().int().min(1).max(10000000),
  coinGeckoMonthlyCap: z.number().int().min(1).max(10000000),
});
export type MonitorSettings = z.infer<typeof monitorSettingsSchema>;
export const defaultMonitorSettings: MonitorSettings = {
  stockSeconds: 30,
  cryptoSeconds: 60,
  goldSeconds: 60,
  coinGeckoMode: "public",
  allTickEnabled: false,
  allTickTransport: "polling",
  allTickSubscriptionCap: 5,
  allTickDailyCap: 1000,
  coinGeckoMonthlyCap: 10000,
};
export const documentSchema = z.object({
  version: z.literal(1),
  revision: z.number().int().min(0),
  accounts: z.array(accountSchema).min(1).max(100),
  instruments: z.array(instrumentSchema).max(10000),
  trades: z.array(tradeSchema).max(100000),
  watchlist: z.array(z.string()).max(10000),
  alerts: z.array(alertSchema).max(1000),
  settings: monitorSettingsSchema,
});
export type InvestmentDocument = z.infer<typeof documentSchema>;
export type PortfolioPosition = {
  accountId: string;
  instrumentId: string;
  quantity: string;
  costCny: string;
  averageCostCny: string;
  realizedCny: string;
  valueCny: string | null;
  gainCny: string | null;
  gainPercent: string | null;
  priceCny: string | null;
  status: MarketQuote["status"] | "missing";
};
export type PortfolioResult = {
  positions: PortfolioPosition[];
  costCny: string;
  knownValueCny: string;
  realizedCny: string;
  knownGainCny: string;
  complete: boolean;
  estimated: boolean;
};
export const kindLabels = {
  stock: "股票",
  etf: "ETF",
  index: "指数",
  gold: "实物黄金",
  crypto: "加密货币",
};
export const sideLabels = {
  buy: "买入",
  sell: "卖出",
  opening: "起始持仓",
  adjust: "持仓校正",
};
export const accountLabels = {
  general: "综合",
  securities: "证券",
  exchange: "交易所",
  wallet: "钱包",
  gold: "黄金",
};
