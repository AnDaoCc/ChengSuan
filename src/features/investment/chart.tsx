import { useEffect, useRef, useState } from "react";
import {
  CandlestickSeries,
  HistogramSeries,
  LineSeries,
  createChart,
  type Time,
  type UTCTimestamp,
} from "lightweight-charts";
import { useInvestment } from "./hooks";
import { movingAverage, normalizeCandles } from "../../core/investment/engine";
import type { Instrument } from "../../core/investment/model";
import type { CandleResult } from "../../infrastructure/investment";
import { price, timestamp } from "./format";
import { Button } from "../../shared/ui/button";

export function InvestmentChart({ asset }: { asset: Instrument }) {
  const { service, document } = useInvestment();
  const container = useRef<HTMLDivElement>(null);
  const [dark, setDark] = useState(documentElementIsDark());
  useEffect(() => {
    const observer = new MutationObserver(() =>
      setDark(documentElementIsDark()),
    );
    observer.observe(window.document.documentElement, {
      attributes: true,
      attributeFilter: ["class"],
    });
    return () => observer.disconnect();
  }, []);
  const [timeframe, setTimeframe] = useState(
    asset.provider === "coingecko" ? "30d" : "day",
  );
  const [adjust, setAdjust] = useState("qfq");
  const [data, setData] = useState<CandleResult | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(true);
  const [retry, setRetry] = useState(0);
  const authorizedMapping =
    asset.provider === "cn" ||
    [
      "bitcoin",
      "ethereum",
      "binancecoin",
      "solana",
      "ripple",
      "dogecoin",
    ].includes(asset.providerId);
  const options =
    asset.provider === "cn"
      ? [
          ["day", "日 K"],
          ["week", "周 K"],
          ["month", "月 K"],
          ...(document?.settings.allTickEnabled
            ? ([["1m", "授权 1 分钟 · 不复权"]] as string[][])
            : []),
          ["5m", "5 分钟"],
          ["15m", "15 分钟"],
          ["60m", "60 分钟"],
        ]
      : asset.provider === "sge"
        ? [["day", "日 K"]]
        : asset.provider === "gecko"
          ? [
              ["day", "日 K"],
              ["1h", "1 小时"],
              ["4h", "4 小时"],
              ["1m", "1 分钟"],
              ["5m", "5 分钟"],
              ["15m", "15 分钟"],
            ]
          : [
              ["1d", "近 1 天 · 30 分钟"],
              ["7d", "近 7 天 · 4 小时"],
              ["30d", "近 30 天 · 4 小时"],
              ["90d", "近 90 天 · 4 日"],
              ...(document?.settings.allTickEnabled && authorizedMapping
                ? ([
                    ["day", "AllTick 日 K"],
                    ["week", "AllTick 周 K"],
                    ["month", "AllTick 月 K"],
                    ["1m", "AllTick 1 分钟"],
                    ["5m", "AllTick 5 分钟"],
                    ["15m", "AllTick 15 分钟"],
                    ["60m", "AllTick 1 小时"],
                    ["4h", "AllTick 4 小时"],
                  ] as string[][])
                : document?.settings.coinGeckoMode === "pro"
                  ? [["day", "授权日 K"]]
                  : []),
            ];
  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      setBusy(true);
      try {
        const response = await service.candles(asset, timeframe, adjust);
        if (!cancelled) {
          setData(response);
          setError("");
        }
      } catch (e) {
        if (!cancelled) {
          setData(null);
          setError(e instanceof Error ? e.message : String(e));
        }
      } finally {
        if (!cancelled) setBusy(false);
      }
    };
    void load();
    const timer = setInterval(() => void load(), 60000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [asset.id, asset.pool?.address, timeframe, adjust, retry, service]);
  useEffect(() => {
    if (!container.current || !data) return;
    const bars = normalizeCandles(data.bars);
    const chart = createChart(container.current, {
      height: 400,
      autoSize: true,
      layout: {
        background: { color: dark ? "#1a1e2d" : "#ffffff" },
        textColor: dark ? "#a0a7bd" : "#7e8497",
        attributionLogo: true,
      },
      grid: {
        vertLines: { color: dark ? "#2c3246" : "#f1f3f8" },
        horzLines: { color: dark ? "#2c3246" : "#f1f3f8" },
      },
      rightPriceScale: {
        borderVisible: false,
        scaleMargins: {
          top: 0.07,
          bottom: data.bars.some((b) => b.volume !== null) ? 0.25 : 0.08,
        },
      },
      timeScale: {
        borderVisible: false,
        timeVisible: !["day", "week", "month", "90d"].includes(timeframe),
        tickMarkFormatter: (t: Time) =>
          new Date(Number(t) * 1000).toLocaleString("zh-CN", {
            timeZone: "Asia/Shanghai",
            month: "2-digit",
            day: "2-digit",
            ...(!["day", "week", "month", "90d"].includes(timeframe)
              ? { hour: "2-digit", minute: "2-digit" }
              : {}),
          }),
      },
      localization: {
        locale: "zh-CN",
        timeFormatter: (t: Time) =>
          new Date(Number(t) * 1000).toLocaleString("zh-CN", {
            timeZone: "Asia/Shanghai",
            hour12: false,
          }),
      },
    });
    const candles = chart.addSeries(CandlestickSeries, {
      upColor: "#df6574",
      downColor: "#38aa88",
      wickUpColor: "#df6574",
      wickDownColor: "#38aa88",
      borderVisible: false,
      priceFormat: {
        type: "custom",
        formatter: (v: number) => price(String(v)),
        minMove: Math.pow(
          10,
          Math.floor(Math.log10(Math.min(...bars.map((b) => Number(b.low))))) -
            5,
        ),
      },
    });
    candles.setData(
      bars.map((b) => ({
        time: b.time as UTCTimestamp,
        open: Number(b.open),
        high: Number(b.high),
        low: Number(b.low),
        close: Number(b.close),
      })),
    );
    for (const [period, color] of [
      [5, "#f3a64d"],
      [10, "#5865f2"],
      [20, "#a78bfa"],
    ] as const) {
      const series = chart.addSeries(LineSeries, {
        color,
        lineWidth: 1,
        title: `MA${period}`,
        priceLineVisible: false,
        lastValueVisible: false,
      });
      series.setData(
        movingAverage(bars, period).map((p) => ({
          time: p.time as UTCTimestamp,
          value: Number(p.value),
        })),
      );
    }
    if (bars.some((b) => b.volume !== null)) {
      const volumes = chart.addSeries(HistogramSeries, {
        priceScaleId: "volume",
        priceFormat: { type: "volume" },
        priceLineVisible: false,
        lastValueVisible: false,
      });
      volumes
        .priceScale()
        .applyOptions({ scaleMargins: { top: 0.82, bottom: 0 } });
      volumes.setData(
        bars
          .filter((b) => b.volume !== null)
          .map((b) => ({
            time: b.time as UTCTimestamp,
            value: Number(b.volume),
            color:
              Number(b.close) >= Number(b.open) ? "#df657455" : "#38aa8855",
          })),
      );
    }
    chart.timeScale().fitContent();
    return () => chart.remove();
  }, [data, timeframe, document?.settings, dark]);
  return (
    <section className="panel investment-chart">
      <div className="chart-toolbar">
        <h3>K 线与均线</h3>
        <select
          aria-label="K线周期"
          value={timeframe}
          onChange={(e) => {
            setTimeframe(e.target.value);
            if (e.target.value === "1m" && asset.provider === "cn")
              setAdjust("none");
          }}
        >
          {options.map(([value, label]) => (
            <option key={value} value={value}>
              {label}
            </option>
          ))}
        </select>
        {asset.provider === "cn" && (
          <select
            aria-label="复权方式"
            disabled={timeframe === "1m"}
            value={adjust}
            onChange={(e) => setAdjust(e.target.value)}
          >
            <option value="qfq">前复权（图表）</option>
            <option value="none">不复权</option>
          </select>
        )}
        <Button
          variant="ghost"
          onClick={() => setRetry((v) => v + 1)}
          disabled={busy}
        >
          {busy ? "加载中…" : "更新图表"}
        </Button>
      </div>
      {error ? (
        <div className="chart-empty">
          <p>{error}</p>
          <Button variant="secondary" onClick={() => setRetry((v) => v + 1)}>
            重试
          </Button>
        </div>
      ) : !data ? (
        <div className="chart-empty">正在获取真实历史行情…</div>
      ) : (
        <div className="chart-container" ref={container} />
      )}
      {data && (
        <div className="chart-caption">
          <span>
            {data.interval} · {data.currency} · {data.source}
          </span>
          <span>
            MA5 / MA10 / MA20 ·{" "}
            {data.volumeUnit === "未提供"
              ? "该源未提供逐根成交量"
              : `成交量单位：${data.volumeUnit}`}
          </span>
          <span>
            {data.cached ? "缓存历史数据" : "最近获取"}：
            {timestamp(data.fetchedAt)}
            {data.cacheReason ? ` · ${data.cacheReason}` : ""}
          </span>
        </div>
      )}
    </section>
  );
}
function documentElementIsDark() {
  return document.documentElement.classList.contains("dark");
}
