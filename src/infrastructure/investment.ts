import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { desktop } from "./desktop";
import {
  documentSchema,
  instrumentSchema,
  quoteSchema,
  candleSchema,
  type InvestmentDocument,
  type Instrument,
  type MarketQuote,
  type AlertState,
  type AlertEvent,
  type Candle,
} from "../core/investment/model";

export type MarketPageResult = {
  items: Instrument[];
  total: number;
  page: number;
  busy: boolean;
  catalogDate: string;
};
export type CandleResult = {
  bars: Candle[];
  source: string;
  interval: string;
  currency: string;
  volumeUnit: string;
  fetchedAt: string;
  cached: boolean;
  cacheReason?: string;
};
export type QuoteEnvelope = {
  quotes: MarketQuote[];
  errors: string[];
  fetchedAt: string;
};
function quotes(raw: {
  quotes: unknown[];
  errors: string[];
  fetchedAt: string;
}): QuoteEnvelope {
  const valid: MarketQuote[] = [];
  const errors = [...raw.errors];
  for (const row of raw.quotes) {
    const result = quoteSchema.safeParse(row);
    if (result.success) valid.push(result.data);
    else errors.push("部分行情字段校验失败，未用于估值");
  }
  return { ...raw, quotes: valid, errors: [...new Set(errors)] };
}
export const investmentRepository = {
  async load() {
    if (!desktop) throw Error("请运行桌面 EXE 使用投资数据库和行情监控");
    const data = await invoke<{
      document: unknown;
      alertStates: { id: string; state: AlertState }[];
      events: AlertEvent[];
    }>("investment_load");
    return { ...data, document: documentSchema.parse(data.document) };
  },
  save: (document: InvestmentDocument) =>
    invoke<number>("investment_save", { document }),
  reportAlerts: (states: Record<string, AlertState>, events: AlertEvent[]) =>
    invoke<void>("investment_alert_report", { states, events }),
  async search(
    category: string,
    query: string,
    page = 1,
    force = false,
  ): Promise<MarketPageResult> {
    const response = await invoke<
      Omit<MarketPageResult, "items"> & { items: unknown[] }
    >("market_search", { category, query, page, force });
    return {
      ...response,
      items: response.items.map((item) => instrumentSchema.parse(item)),
    };
  },
  visible: (instruments: Instrument[]) =>
    invoke<void>("market_visible", { instruments }),
  async snapshot() {
    return quotes(
      await invoke<{ quotes: unknown[]; errors: string[]; fetchedAt: string }>(
        "market_snapshot",
      ),
    );
  },
  refresh: () => invoke<void>("market_refresh"),
  async resolve(chain: string, contract: string) {
    return instrumentSchema.parse(
      await invoke("market_resolve", { chain, contract }),
    );
  },
  fx: (currency: string, date: string) =>
    invoke<{ rate: string; date: string; source: string }>("market_fx", {
      currency,
      date,
    }),
  async candles(
    instrument: Instrument,
    timeframe: string,
    adjust: string,
  ): Promise<CandleResult> {
    const data = await invoke<Omit<CandleResult, "bars"> & { bars: unknown[] }>(
      "market_candles",
      { instrument, timeframe, adjust },
    );
    return { ...data, bars: data.bars.map((bar) => candleSchema.parse(bar)) };
  },
  secretStatus: () =>
    invoke<{ allTick: boolean; coinGecko: boolean; error?: string }>(
      "market_secret_status",
    ),
  secretSave: (patch: Record<string, string>) =>
    invoke<{ allTick: boolean; coinGecko: boolean }>("market_secret_save", {
      patch,
    }),
  quoteListener: async (callback: (data: QuoteEnvelope) => void) =>
    listen<{ quotes: unknown[]; errors: string[]; fetchedAt: string }>(
      "investment:quotes",
      (event) => callback(quotes(event.payload)),
    ),
  directoryListener: async (
    callback: (data: { busy: boolean; message: string }) => void,
  ) =>
    listen<{ busy: boolean; message: string }>(
      "investment:directory",
      (event) => callback(event.payload),
    ),
  restoreListener: async (callback: () => void) =>
    listen("investment:restored", callback),
};
