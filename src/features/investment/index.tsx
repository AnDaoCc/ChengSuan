import { useEffect, useMemo, useState } from "react";
import {
  ArrowLeft,
  ArrowUpRight,
  Bell,
  Check,
  ChevronLeft,
  ChevronRight,
  Coins,
  Edit3,
  Plus,
  RefreshCw,
  Search,
  Settings2,
  Star,
  Trash2,
  Wallet,
  X,
} from "lucide-react";
import { useInvestment } from "./hooks";
import { investmentService } from "./service";
import { D, portfolio } from "../../core/investment/engine";
import {
  alertSchema,
  monitorSettingsSchema,
  kindLabels,
  sideLabels,
  accountLabels,
  type Instrument,
  type Trade,
  type AlertRule,
} from "../../core/investment/model";
import type { MarketPageResult } from "../../infrastructure/investment";
import { Button } from "../../shared/ui/button";
import { Modal } from "../../shared/ui/dialog";
import { Field } from "../../shared/ui/fields";
import { errorText } from "../../shared/utils";
import { cash, price, signed, timestamp, quoteStatus } from "./format";
import { AccountForm, AssetPicker, AssetSettings, TradeForm } from "./forms";
import { InvestmentChart } from "./chart";
export { investmentService };

const categories = [
  ["index", "主要指数"],
  ["SH", "沪市"],
  ["SZ", "深市"],
  ["BJ", "北交所"],
  ["etf", "普通 ETF"],
  ["goldEtf", "黄金 ETF"],
  ["gold", "实物黄金"],
  ["crypto", "加密货币"],
] as const;
const alertLabels = {
  priceAbove: "价格达到上限（人民币）",
  priceBelow: "价格达到下限（人民币）",
  gainAbove: "持仓盈利金额达到",
  gainBelow: "持仓盈亏金额低于",
  percentAbove: "持仓盈利百分比达到",
  percentBelow: "持仓盈亏百分比低于",
};
const marketLabels = {
  SH: "上海",
  SZ: "深圳",
  BJ: "北京",
  SGE: "上海黄金交易所",
  CRYPTO: "加密市场",
};
export function InvestmentPage() {
  const state = useInvestment();
  const { document: doc, service, quotes, directoryVersion } = state;
  const [tab, setTab] = useState("market");
  const [category, setCategory] = useState("index");
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(1);
  const [listing, setListing] = useState<MarketPageResult | null>(null);
  const [marketError, setMarketError] = useState("");
  const [searching, setSearching] = useState(false);
  const [account, setAccount] = useState("all");
  const [detail, setDetail] = useState<Instrument | null>(null);
  const [picker, setPicker] = useState(false);
  const [trade, setTrade] = useState<{
    asset: Instrument;
    existing?: Trade;
    side?: Trade["side"];
  } | null>(null);
  const [accountForm, setAccountForm] = useState<string | null>(null);
  const [token, setToken] = useState(false);
  const [assetSettings, setAssetSettings] = useState<Instrument | null>(null);
  const [alertForm, setAlertForm] = useState<Instrument | null>(null);
  const currentAsset =
    detail && (doc?.instruments.find((a) => a.id === detail.id) ?? detail);
  const quoteMap = useMemo(
    () => new Map(quotes.map((q) => [q.instrumentId, q])),
    [quotes],
  );
  const result = useMemo(
    () =>
      doc
        ? portfolio(doc, quotes, account === "all" ? undefined : account)
        : null,
    [doc, quotes, account],
  );
  const rows = useMemo(
    () =>
      tab === "watch"
        ? (doc?.instruments.filter(
            (a) =>
              doc.watchlist.includes(a.id) &&
              `${a.name} ${a.symbol} ${a.contract ?? ""}`
                .toLowerCase()
                .includes(query.toLowerCase()),
          ) ?? [])
        : (listing?.items ?? []),
    [tab, doc, query, listing],
  );
  useEffect(() => {
    if (tab !== "market" || detail) return;
    let cancel = false;
    const timer = setTimeout(() => {
      setSearching(true);
      void service
        .search(category, query, page)
        .then((data) => {
          if (!cancel) {
            setListing(data);
            setMarketError("");
          }
        })
        .catch((e) => {
          if (!cancel) setMarketError(errorText(e));
        })
        .finally(() => {
          if (!cancel) setSearching(false);
        });
    }, 300);
    return () => {
      cancel = true;
      clearTimeout(timer);
    };
  }, [tab, category, query, page, directoryVersion, detail, service]);
  useEffect(() => {
    const assets = currentAsset
      ? [currentAsset]
      : tab === "market" || tab === "watch"
        ? rows
        : [];
    void service
      .visible(
        assets.map(
          (a) => doc?.instruments.find((saved) => saved.id === a.id) ?? a,
        ),
      )
      .catch(service.notify);
    return () => {
      void service.visible([]).catch(service.notify);
    };
  }, [currentAsset?.id, currentAsset?.pool?.address, tab, rows, service]);
  const openTrade = (
    asset: Instrument,
    side: Trade["side"] = "buy",
    existing?: Trade,
  ) => setTrade({ asset, side, existing });
  const action = (promise: Promise<unknown>) =>
    void promise.catch(service.notify);
  if (state.loading)
    return (
      <div className="empty-state">
        <RefreshCw className="spinning" />
        <p>正在打开投资账户…</p>
      </div>
    );
  if (!doc || !result)
    return (
      <div className="warning">
        {state.error || "投资数据尚未加载"}
        <Button onClick={() => void service.reload()}>重试</Button>
      </div>
    );
  return (
    <div className="investment-page">
      <header className="page-header">
        <div>
          <div className="eyebrow">INVESTMENT WORKSPACE</div>
          <h1>投资与持仓</h1>
          <p>看行情，记交易，跟踪每一份持仓。</p>
        </div>
        <div className="investment-header-actions">
          <span className="investment-save">
            {state.saving ? "正在保存…" : "投资记录已保存"}
          </span>
          <Button variant="secondary" onClick={() => action(service.refresh())}>
            <RefreshCw size={16} />
            刷新行情
          </Button>
        </div>
      </header>
      <div className="investment-tabs" role="tablist" aria-label="投资页面">
        {[
          ["market", "行情市场"],
          ["watch", "我的自选"],
          ["portfolio", "我的持仓"],
          ["trades", "交易记录"],
          ["alerts", "监控提醒"],
          ["sources", "行情设置"],
        ].map(([key, label]) => (
          <button
            role="tab"
            aria-selected={tab === key && !detail}
            className={tab === key && !detail ? "selected" : ""}
            key={key}
            onClick={() => {
              setDetail(null);
              setTab(key);
              setQuery("");
              setPage(1);
            }}
          >
            {label}
          </button>
        ))}
      </div>
      {state.errors.length > 0 && (
        <details className="investment-network">
          <summary>部分行情获取未成功 · 保留最近有效报价</summary>
          {state.errors.map((error, i) => (
            <p key={i}>{error}</p>
          ))}
        </details>
      )}
      {currentAsset ? (
        <>
          <button className="back-link" onClick={() => setDetail(null)}>
            <ArrowLeft size={16} />
            返回
            {tab === "watch" ? "自选" : tab === "portfolio" ? "持仓" : "行情"}
          </button>
          <section className="panel asset-detail">
            <div>
              <span className="badge">
                {kindLabels[currentAsset.kind]} ·{" "}
                {marketLabels[currentAsset.market]}
              </span>
              <h2>
                {currentAsset.name} <small>{currentAsset.symbol}</small>
              </h2>
              <p className="muted">
                {currentAsset.chain && `${currentAsset.chain} · `}
                {currentAsset.contract ?? currentAsset.providerId}
              </p>
              {currentAsset.pool && (
                <p className="muted">
                  交易池：{currentAsset.pool.name} · 目标币在{" "}
                  {currentAsset.pool.tokenSide === "base" ? "基础币" : "计价币"}
                  侧
                </p>
              )}
            </div>
            <QuoteDisplay asset={currentAsset} />
            <div className="asset-actions">
              <Button
                variant="secondary"
                onClick={() => action(service.watch(currentAsset))}
              >
                <Star
                  size={16}
                  fill={
                    doc.watchlist.includes(currentAsset.id)
                      ? "currentColor"
                      : "none"
                  }
                />
                {doc.watchlist.includes(currentAsset.id)
                  ? "移出自选"
                  : "加入自选"}
              </Button>
              {currentAsset.kind !== "index" && (
                <Button onClick={() => openTrade(currentAsset)}>
                  <Plus size={16} />
                  记录交易
                </Button>
              )}
              <Button
                variant="ghost"
                onClick={() => setAlertForm(currentAsset)}
              >
                <Bell size={16} />
                设置提醒
              </Button>
              <Button
                variant="ghost"
                onClick={() => setAssetSettings(currentAsset)}
              >
                <Settings2 size={16} />
                估值设置
              </Button>
            </div>
          </section>
          <InvestmentChart key={currentAsset.id} asset={currentAsset} />
          {currentAsset.kind === "gold" && (
            <p className="muted">
              Au99.99：人民币／克。纯度 {currentAsset.purity}
              ，参考估值不包含品牌溢价、回购价差。夜盘显示实际报价时间及源交易日。
            </p>
          )}
          <p className="market-notice">
            行情为参考数据，可能延迟。持仓使用实际未复权报价；图表复权不会修改成交价格。
          </p>
        </>
      ) : tab === "market" || tab === "watch" ? (
        <>
          <div className="market-toolbar">
            <div className="investment-search">
              <Search size={17} />
              <input
                aria-label="检索行情"
                value={query}
                onChange={(e) => {
                  setQuery(e.target.value);
                  setPage(1);
                }}
                placeholder="中文名称、代码、币种、交易对或已添加合约…"
              />
            </div>
            <Button variant="secondary" onClick={() => setToken(true)}>
              <Coins size={16} />
              添加合约币
            </Button>
          </div>
          {tab === "market" && (
            <div className="market-categories">
              {categories.map(([key, label]) => (
                <button
                  key={key}
                  className={category === key ? "selected" : ""}
                  onClick={() => {
                    setCategory(key);
                    setPage(1);
                  }}
                >
                  {label}
                </button>
              ))}
            </div>
          )}
          {state.directoryBusy && (
            <div className="muted directory-progress">
              <RefreshCw size={14} className="spinning" />
              {state.directoryMessage}
            </div>
          )}
          {marketError && <p className="warning">{marketError}</p>}
          <section className="panel market-table">
            <div className="table-scroll">
              <table>
                <thead>
                  <tr>
                    <th>资产 / 代码</th>
                    <th>类别</th>
                    <th>最新参考单价</th>
                    <th>涨跌幅</th>
                    <th>报价依据</th>
                    <th>自选</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((raw) => {
                    const a =
                      doc.instruments.find((saved) => saved.id === raw.id) ??
                      raw;
                    const q = quoteMap.get(a.id);
                    return (
                      <tr key={a.id}>
                        <td>
                          <button
                            className="asset-link"
                            onClick={() => setDetail(a)}
                          >
                            <strong>{a.name}</strong>
                            <span>
                              {a.symbol}
                              {a.chain
                                ? ` · ${a.chain}`
                                : ` · ${marketLabels[a.market]}`}
                            </span>
                          </button>
                        </td>
                        <td>{a.goldEtf ? "黄金 ETF" : kindLabels[a.kind]}</td>
                        <td>
                          {a.manualPrice ? (
                            <>
                              <strong>¥ {price(a.manualPrice)}</strong>
                              <small>手动参考价</small>
                            </>
                          ) : q ? (
                            <>
                              <strong>
                                {price(q.price)}{" "}
                                <small>
                                  {q.currency}/{a.unit}
                                </small>
                              </strong>
                              <small>{quoteStatus[q.status]}</small>
                            </>
                          ) : (
                            <span className="muted">等待报价</span>
                          )}
                        </td>
                        <td
                          className={
                            q?.changePercent && new D(q.changePercent).gte(0)
                              ? "gain-positive"
                              : "gain-negative"
                          }
                        >
                          {q?.changePercent
                            ? `${signed(q.changePercent)}%`
                            : "—"}
                        </td>
                        <td className="quote-evidence">
                          <span>{q?.source ?? "尚未获取"}</span>
                          <small>
                            {q?.sourceAt
                              ? timestamp(q.sourceAt)
                              : "报价时间未提供"}
                          </small>
                        </td>
                        <td>
                          <button
                            className="icon-button"
                            title={
                              doc.watchlist.includes(a.id)
                                ? "移出自选"
                                : "加入自选"
                            }
                            aria-label={`${doc.watchlist.includes(a.id) ? "移出" : "加入"}${a.name}自选`}
                            onClick={() => action(service.watch(a))}
                          >
                            <Star
                              size={18}
                              fill={
                                doc.watchlist.includes(a.id)
                                  ? "currentColor"
                                  : "none"
                              }
                            />
                          </button>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
            {!rows.length && (
              <div className="investment-empty">
                {searching
                  ? "正在检索…"
                  : tab === "watch"
                    ? "还没有自选，在行情列表点击星标添加。"
                    : listing?.busy
                      ? "正在建立完整股票与 ETF 目录…"
                      : "没有匹配的标的，合约币请使用“添加合约币”。"}
              </div>
            )}
            {tab === "market" && (
              <div className="market-pagination">
                <span>
                  {searching ? "搜索中…" : `共 ${listing?.total ?? 0} 项`} ·
                  目录 {listing?.catalogDate || "待更新"}
                </span>
                <div>
                  <Button
                    variant="ghost"
                    disabled={page <= 1 || searching}
                    onClick={() => setPage((p) => p - 1)}
                  >
                    <ChevronLeft size={16} />
                  </Button>
                  <span>
                    {page} /{" "}
                    {Math.max(1, Math.ceil((listing?.total ?? 0) / 40))}
                  </span>
                  <Button
                    variant="ghost"
                    disabled={page * 40 >= (listing?.total ?? 0) || searching}
                    onClick={() => setPage((p) => p + 1)}
                  >
                    <ChevronRight size={16} />
                  </Button>
                  <Button
                    variant="ghost"
                    disabled={state.directoryBusy}
                    onClick={() =>
                      action(
                        service
                          .search(category, query, page, true)
                          .then(setListing),
                      )
                    }
                  >
                    更新目录
                  </Button>
                </div>
              </div>
            )}
          </section>
          <p className="market-notice">
            股票 /
            ETF：东方财富目录与历史、腾讯报价补充；黄金：上海黄金交易所；币种：CoinGecko
            / GeckoTerminal。免费参考源可能延迟、限流或中断。
          </p>
        </>
      ) : tab === "portfolio" || tab === "trades" ? (
        <>
          <div className="portfolio-toolbar">
            <Field label="账户范围">
              <select
                value={account}
                onChange={(e) => setAccount(e.target.value)}
              >
                <option value="all">所有账户 · 人民币汇总</option>
                {doc.accounts.map((a) => (
                  <option key={a.id} value={a.id}>
                    {a.name} · {accountLabels[a.kind]}
                  </option>
                ))}
              </select>
            </Field>
            <div>
              <Button
                variant="ghost"
                onClick={() =>
                  setAccountForm(account === "all" ? "new" : account)
                }
              >
                <Wallet size={16} />
                {account === "all" ? "新增账户" : "编辑账户"}
              </Button>
              {account !== "all" && (
                <Button
                  variant="ghost"
                  onClick={() => {
                    if (doc.accounts.length === 1) {
                      service.notify("至少保留一个投资账户");
                      return;
                    }
                    if (doc.trades.some((t) => t.accountId === account)) {
                      service.notify("这个账户还有交易记录，不能删除");
                      return;
                    }
                    if (confirm("确认删除这个空账户？"))
                      action(
                        service
                          .edit((d) => {
                            d.accounts = d.accounts.filter(
                              (a) => a.id !== account,
                            );
                            d.alerts = d.alerts.filter(
                              (r) => r.accountId !== account,
                            );
                          })
                          .then(() => setAccount("all")),
                      );
                  }}
                >
                  <Trash2 size={16} />
                  删除空账户
                </Button>
              )}
              <Button onClick={() => setPicker(true)}>
                <Plus size={16} />
                记录交易
              </Button>
            </div>
          </div>
          {tab === "portfolio" ? (
            <>
              <div className="portfolio-summary">
                <div className="panel">
                  <span>当前持仓市值</span>
                  <strong>
                    {cash(
                      !result.complete &&
                        !result.positions.some(
                          (p) => p.valueCny !== null && new D(p.quantity).gt(0),
                        )
                        ? null
                        : result.knownValueCny,
                    )}
                  </strong>
                  <small>
                    {!result.complete
                      ? "已知部分 · 估值未完整"
                      : result.estimated
                        ? "包含缓存或手动参考估值"
                        : "人民币汇总"}
                  </small>
                </div>
                <div className="panel">
                  <span>剩余持仓成本</span>
                  <strong>{cash(result.costCny)}</strong>
                  <small>买入费用计入成本</small>
                </div>
                <div className="panel">
                  <span>
                    未实现盈亏{!result.complete ? "（已知部分）" : ""}
                  </span>
                  <strong
                    className={
                      new D(result.knownGainCny).gte(0)
                        ? "gain-positive"
                        : "gain-negative"
                    }
                  >
                    {cash(
                      !result.complete &&
                        !result.positions.some(
                          (p) => p.valueCny !== null && new D(p.quantity).gt(0),
                        )
                        ? null
                        : result.knownGainCny,
                    )}
                  </strong>
                  <small>持仓市值 − 对应剩余成本</small>
                </div>
                <div className="panel">
                  <span>累计已实现收益</span>
                  <strong
                    className={
                      new D(result.realizedCny).gte(0)
                        ? "gain-positive"
                        : "gain-negative"
                    }
                  >
                    {cash(result.realizedCny)}
                  </strong>
                  <small>已扣除卖出手续费</small>
                </div>
              </div>
              {!result.complete && (
                <div className="warning">
                  部分资产缺报价或人民币换算依据，当前展示可计算的部分，缺失价格没有按
                  0 计入。
                </div>
              )}
              <section className="panel holdings-table">
                <div className="section-title">
                  <h2>持仓明细</h2>
                  <small className="muted">
                    市值不包含卖出后留在外部账户的现金
                  </small>
                </div>
                <div className="table-scroll">
                  <table>
                    <thead>
                      <tr>
                        <th>资产 / 账户</th>
                        <th>数量 / 平均成本</th>
                        <th>剩余成本</th>
                        <th>持仓市值</th>
                        <th>未实现盈亏</th>
                        <th>已实现收益</th>
                        <th>占比</th>
                        <th>操作</th>
                      </tr>
                    </thead>
                    <tbody>
                      {result.positions
                        .filter((p) => new D(p.quantity).gt(0))
                        .map((p) => {
                          const a = doc.instruments.find(
                            (i) => i.id === p.instrumentId,
                          )!;
                          const proportion =
                            p.valueCny && new D(result.knownValueCny).gt(0)
                              ? new D(p.valueCny)
                                  .div(result.knownValueCny)
                                  .mul(100)
                                  .toFixed(2)
                              : null;
                          return (
                            <tr key={`${p.accountId}:${p.instrumentId}`}>
                              <td>
                                <button
                                  className="asset-link"
                                  onClick={() => setDetail(a)}
                                >
                                  <strong>{a.name}</strong>
                                  <span>
                                    {a.symbol} ·{" "}
                                    {
                                      doc.accounts.find(
                                        (ac) => ac.id === p.accountId,
                                      )?.name
                                    }
                                  </span>
                                </button>
                                <small>
                                  {quoteStatus[
                                    p.status as keyof typeof quoteStatus
                                  ] ?? "缺报价"}
                                  {a.kind === "gold"
                                    ? ` · 纯度 ${a.purity}`
                                    : ""}
                                </small>
                              </td>
                              <td>
                                <strong>
                                  {price(p.quantity)} {a.unit}
                                </strong>
                                <small>
                                  {cash(p.averageCostCny)} / {a.unit}
                                </small>
                              </td>
                              <td>{cash(p.costCny)}</td>
                              <td>{cash(p.valueCny)}</td>
                              <td
                                className={
                                  p.gainCny && new D(p.gainCny).gte(0)
                                    ? "gain-positive"
                                    : "gain-negative"
                                }
                              >
                                {cash(p.gainCny)}
                                <small>
                                  {p.gainPercent === null
                                    ? "—"
                                    : `${signed(p.gainPercent)}%`}
                                </small>
                              </td>
                              <td>{cash(p.realizedCny)}</td>
                              <td>
                                {proportion ? `${proportion}%` : "—"}
                                <small>
                                  {!result.complete ? "已知市值占比" : ""}
                                </small>
                              </td>
                              <td>
                                <div className="row-actions">
                                  <button
                                    onClick={() => {
                                      setAccount(p.accountId);
                                      openTrade(a);
                                    }}
                                  >
                                    买入
                                  </button>
                                  <button
                                    onClick={() => {
                                      setAccount(p.accountId);
                                      openTrade(a, "sell");
                                    }}
                                  >
                                    卖出
                                  </button>
                                  <button
                                    onClick={() => {
                                      setAccount(p.accountId);
                                      openTrade(a, "adjust");
                                    }}
                                  >
                                    校正
                                  </button>
                                </div>
                              </td>
                            </tr>
                          );
                        })}
                    </tbody>
                  </table>
                </div>
                {!result.positions.some((p) => new D(p.quantity).gt(0)) && (
                  <div className="investment-empty">
                    暂无持仓，记录买入交易或起始持仓后会自动计算。
                  </div>
                )}
              </section>
              <p className="muted">
                移动加权平均成本；按成交时间重算。分红、送股、拆合股及账户间转移请用“持仓校正”并填写依据。
              </p>
            </>
          ) : (
            <section className="panel">
              <div className="section-title">
                <h2>逐笔交易记录</h2>
                <span className="muted">历史编辑会重新核算后续持仓</span>
              </div>
              <div className="table-scroll">
                <table>
                  <thead>
                    <tr>
                      <th>时间 / 账户</th>
                      <th>资产 / 类型</th>
                      <th>成交数量</th>
                      <th>单价 / 费用</th>
                      <th>冻结换算依据</th>
                      <th>备注</th>
                      <th>操作</th>
                    </tr>
                  </thead>
                  <tbody>
                    {doc.trades
                      .filter(
                        (t) => account === "all" || t.accountId === account,
                      )
                      .sort(
                        (a, b) =>
                          b.executedAt.localeCompare(a.executedAt) ||
                          b.sequence - a.sequence,
                      )
                      .map((t) => {
                        const a = doc.instruments.find(
                          (i) => i.id === t.instrumentId,
                        )!;
                        return (
                          <tr key={t.id}>
                            <td>
                              {timestamp(t.executedAt)}
                              <small>
                                {
                                  doc.accounts.find(
                                    (ac) => ac.id === t.accountId,
                                  )?.name
                                }
                              </small>
                            </td>
                            <td>
                              <strong>{a.name}</strong>
                              <small>
                                {a.symbol} · {sideLabels[t.side]}
                              </small>
                            </td>
                            <td>
                              {price(t.quantity)} {a.unit}
                            </td>
                            <td>
                              {price(t.unitPrice)} {t.currency}
                              <small>
                                费用 {price(t.fee)} {t.currency}
                              </small>
                            </td>
                            <td>
                              1 {t.currency} = {price(t.fxToCny)} CNY
                              <small>
                                {t.fxDate} · {t.fxSource}
                              </small>
                            </td>
                            <td className="trade-note">{t.note || "—"}</td>
                            <td>
                              <div className="row-actions">
                                <button
                                  aria-label={`编辑${a.name}交易`}
                                  onClick={() => openTrade(a, t.side, t)}
                                >
                                  <Edit3 size={16} />
                                </button>
                                <button
                                  aria-label={`删除${a.name}交易`}
                                  onClick={() => {
                                    if (
                                      confirm(
                                        "确认删除这笔交易？之后的持仓会重新计算，如出现超额卖出则禁止删除。",
                                      )
                                    )
                                      action(service.deleteTrade(t.id));
                                  }}
                                >
                                  <Trash2 size={16} />
                                </button>
                              </div>
                            </td>
                          </tr>
                        );
                      })}
                  </tbody>
                </table>
              </div>
              {!doc.trades.length && (
                <div className="investment-empty">尚无交易记录。</div>
              )}
            </section>
          )}
        </>
      ) : tab === "alerts" ? (
        <>
          <div className="section-title">
            <div>
              <h2>监控规则</h2>
              <p className="muted">
                软件运行期间持续监控，包括最小化及切换页面。缓存和手动估价不触发实时提醒。
              </p>
            </div>
            <Button onClick={() => setPicker(true)}>
              <Plus size={16} />
              选择资产设提醒
            </Button>
          </div>
          <section className="panel alert-rules">
            {doc.alerts.map((rule) => {
              const a = doc.instruments.find(
                (i) => i.id === rule.instrumentId,
              )!;
              return (
                <div className="alert-rule" key={rule.id}>
                  <Bell size={20} />
                  <div>
                    <strong>
                      {a.name} · {alertLabels[rule.kind]} {rule.threshold}
                      {rule.kind.startsWith("percent") ? "%" : " 元"}
                    </strong>
                    <small>
                      {rule.accountId
                        ? doc.accounts.find((ac) => ac.id === rule.accountId)
                            ?.name
                        : "全部账户"}{" "}
                      · 冷却 {rule.cooldownMinutes} 分钟
                    </small>
                  </div>
                  <Button
                    variant="ghost"
                    onClick={() =>
                      action(
                        service.edit((d) => {
                          d.alerts = d.alerts.map((r) =>
                            r.id === rule.id
                              ? { ...r, enabled: !r.enabled }
                              : r,
                          );
                        }),
                      )
                    }
                  >
                    {rule.enabled ? "已启用" : "已暂停"}
                  </Button>
                  <button
                    className="icon-button"
                    aria-label="删除提醒规则"
                    onClick={() => {
                      if (confirm("确认删除这条监控规则？"))
                        action(
                          service.edit((d) => {
                            d.alerts = d.alerts.filter((r) => r.id !== rule.id);
                          }),
                        );
                    }}
                  >
                    <Trash2 size={16} />
                  </button>
                </div>
              );
            })}
            {!doc.alerts.length && (
              <div className="investment-empty">
                还没有监控规则，可以从资产详情设置。
              </div>
            )}
          </section>
          <section className="panel alert-history">
            <h2>提醒记录</h2>
            {state.events.map((event) => (
              <div key={event.id}>
                <Bell size={16} />
                <span>
                  {event.message}
                  <small>{timestamp(event.triggeredAt)}</small>
                </span>
              </div>
            ))}
            {!state.events.length && (
              <p className="muted">
                尚未触发提醒。新有效报价首次达到或重新越过阈值才会提醒。
              </p>
            )}
          </section>
        </>
      ) : (
        <SourceSettings />
      )}
      {picker && (
        <AssetPicker
          onClose={() => setPicker(false)}
          onSelect={(asset) => {
            setPicker(false);
            if (tab === "alerts") setAlertForm(asset);
            else openTrade(asset);
          }}
        />
      )}
      {trade && (
        <TradeForm
          asset={trade.asset}
          existing={trade.existing}
          initialSide={trade.side}
          accountId={account === "all" ? undefined : account}
          onClose={() => setTrade(null)}
        />
      )}
      {accountForm && (
        <AccountForm
          existing={doc.accounts.find((a) => a.id === accountForm)}
          onClose={() => setAccountForm(null)}
        />
      )}
      {token && (
        <TokenForm
          onClose={() => setToken(false)}
          onAdded={(asset) => {
            setToken(false);
            setDetail(asset);
          }}
        />
      )}
      {assetSettings && (
        <AssetSettings
          asset={assetSettings}
          onClose={() => setAssetSettings(null)}
        />
      )}
      {alertForm && (
        <AlertForm asset={alertForm} onClose={() => setAlertForm(null)} />
      )}
    </div>
  );
}

function QuoteDisplay({ asset }: { asset: Instrument }) {
  const { quotes } = useInvestment();
  const q = quotes.find((row) => row.instrumentId === asset.id);
  return (
    <div className="asset-quote">
      <strong>
        {asset.manualPrice
          ? `¥ ${price(asset.manualPrice)}`
          : q
            ? `${price(q.price)} ${q.currency}`
            : "等待报价"}
      </strong>
      <span>
        每{asset.unit} ·{" "}
        {asset.manualPrice
          ? "手动参考价"
          : q
            ? quoteStatus[q.status]
            : "暂无有效价格"}
      </span>
      {q && (
        <>
          <small>
            {q.source} · 报价 {timestamp(q.sourceAt)}
          </small>
          <small>
            获取 {timestamp(q.fetchedAt)}
            {q.currency !== "CNY"
              ? ` · 1 ${q.currency} = ${q.cnyRate ? price(q.cnyRate) : "缺汇率"} CNY · ${q.fxDate}`
              : ""}
          </small>
          {q.note && <small>{q.note}</small>}
        </>
      )}
    </div>
  );
}
function TokenForm({
  onClose,
  onAdded,
}: {
  onClose: () => void;
  onAdded: (asset: Instrument) => void;
}) {
  const { service } = useInvestment();
  const [chain, setChain] = useState("eth");
  const [contract, setContract] = useState("");
  const [asset, setAsset] = useState<Instrument | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const resolve = async () => {
    setBusy(true);
    setAsset(null);
    try {
      setAsset(await service.resolve(chain, contract.trim()));
      setError("");
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal
      open
      onOpenChange={(v) => {
        if (!v) onClose();
      }}
      title="读取自定义合约币"
      description="用公链与合约地址识别资产，请核对名称、符号和交易池后加入自选。"
    >
      <div className="form-grid">
        <Field
          label="公链 ID"
          hint="GeckoTerminal 网络 ID，例如 eth、bsc、polygon_pos、solana、base、arbitrum。"
        >
          <input
            value={chain}
            onChange={(e) => {
              setChain(e.target.value);
              setAsset(null);
            }}
            list="chain-ids"
          />
          <datalist id="chain-ids">
            {[
              "eth",
              "bsc",
              "polygon_pos",
              "solana",
              "base",
              "arbitrum",
              "avax",
              "optimism",
            ].map((c) => (
              <option key={c} value={c} />
            ))}
          </datalist>
        </Field>
        <Field label="合约地址">
          <input
            value={contract}
            onChange={(e) => {
              setContract(e.target.value);
              setAsset(null);
            }}
            placeholder="粘贴合约地址"
          />
        </Field>
      </div>
      <Button
        disabled={busy || !contract || !chain}
        variant="secondary"
        onClick={() => void resolve()}
      >
        <Search size={16} />
        {busy ? "正在读取…" : "读取币种与交易池"}
      </Button>
      {error && <p className="warning">{error}</p>}
      {asset && (
        <div className="token-preview">
          <h3>
            {asset.name} · {asset.symbol}
          </h3>
          <p className="muted">
            精度 {asset.decimals ?? "未提供"} · {asset.chain} · {asset.contract}
          </p>
          <Field label="估值交易池（默认流动性最高）">
            <select
              value={asset.pool?.address}
              onChange={(e) =>
                setAsset({
                  ...asset,
                  pool: asset.pools.find((p) => p.address === e.target.value),
                })
              }
            >
              {asset.pools.map((p) => (
                <option key={p.address} value={p.address}>
                  {p.name} · ${price(p.liquidity)} · {p.tokenSide}
                </option>
              ))}
            </select>
          </Field>
          <p className="muted">
            目标币在池的{" "}
            {asset.pool?.tokenSide === "base" ? "基础币" : "计价币"}
            侧，报价采用该侧的美元价格。
          </p>
          <Button
            disabled={busy}
            onClick={() => {
              setBusy(true);
              void service
                .watch(asset)
                .then(() => onAdded(asset))
                .catch((e) => setError(errorText(e)))
                .finally(() => setBusy(false));
            }}
          >
            <Check size={16} />
            确认加入自选
          </Button>
        </div>
      )}
    </Modal>
  );
}
function AlertForm({
  asset,
  onClose,
}: {
  asset: Instrument;
  onClose: () => void;
}) {
  const { document: doc, service } = useInvestment();
  const [rule, setRule] = useState<AlertRule>({
    id: crypto.randomUUID(),
    instrumentId: asset.id,
    accountId: null,
    kind: "priceAbove",
    threshold: "",
    enabled: true,
    cooldownMinutes: 15,
  });
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const save = async () => {
    setBusy(true);
    try {
      const parsed = alertSchema.parse(rule);
      if (rule.kind.startsWith("price") && new D(rule.threshold).lte(0))
        throw Error("价格阈值必须大于 0");
      await service.edit((d) => {
        if (!d.instruments.some((a) => a.id === asset.id))
          d.instruments.push(asset);
        d.alerts.push(parsed);
      });
      onClose();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal
      open
      title={`设置提醒 · ${asset.name}`}
      onOpenChange={(v) => {
        if (!v) onClose();
      }}
      description="仅应用内提醒，软件关闭后停止监控；重新越过阈值且满足冷却时间时再次提醒。"
    >
      <div className="form-grid">
        <Field label="提醒条件">
          <select
            value={rule.kind}
            onChange={(e) =>
              setRule({ ...rule, kind: e.target.value as AlertRule["kind"] })
            }
          >
            {Object.entries(alertLabels).map(([key, label]) => (
              <option key={key} value={key}>
                {label}
              </option>
            ))}
          </select>
        </Field>
        <Field
          label={
            rule.kind.startsWith("percent")
              ? "阈值（百分比，可填负数）"
              : "阈值（人民币元）"
          }
          hint="例如亏损超过 500 元，选择盈亏金额低于并填 -500。"
        >
          <input
            value={rule.threshold}
            onChange={(e) => setRule({ ...rule, threshold: e.target.value })}
          />
        </Field>
        <Field label="持仓账户范围">
          <select
            value={rule.accountId ?? ""}
            onChange={(e) =>
              setRule({ ...rule, accountId: e.target.value || null })
            }
          >
            <option value="">全部账户</option>
            {doc!.accounts.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
          </select>
        </Field>
        <Field label="冷却时间（分钟）">
          <input
            type="number"
            min={1}
            max={1440}
            value={rule.cooldownMinutes}
            onChange={(e) =>
              setRule({ ...rule, cooldownMinutes: Number(e.target.value) })
            }
          />
        </Field>
      </div>
      {error && <p className="warning">{error}</p>}
      <div className="modal-footer">
        <Button disabled={busy} onClick={() => void save()}>
          保存监控规则
        </Button>
      </div>
    </Modal>
  );
}
function SourceSettings() {
  const { document: doc, service } = useInvestment();
  const [settings, setSettings] = useState(doc!.settings);
  const [allTick, setAllTick] = useState("");
  const [cgKey, setCgKey] = useState("");
  const [status, setStatus] = useState<{
    allTick: boolean;
    coinGecko: boolean;
    error?: string;
  }>({ allTick: false, coinGecko: false });
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  useEffect(() => {
    void service.secrets().then(setStatus).catch(service.notify);
  }, [service]);
  const save = async () => {
    setBusy(true);
    try {
      const parsed = monitorSettingsSchema.parse(settings);
      const patch: Record<string, string> = {};
      if (allTick.trim()) patch.allTick = allTick.trim();
      if (cgKey.trim()) patch.coinGecko = cgKey.trim();
      if (Object.keys(patch).length)
        setStatus(await service.saveSecrets(patch));
      await service.edit((d) => {
        d.settings = parsed;
      });
      setAllTick("");
      setCgKey("");
      setMessage("行情设置已保存，新的频率与源将在后台生效。");
    } catch (e) {
      setMessage(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <section className="panel investment-source-settings">
        <h2>监控频率与用量</h2>
        <p className="muted">
          仅刷新持仓、自选、启用提醒和当前可见资产。股票按交易时段刷新；币种及黄金持续尝试，遇限流自动退避。
        </p>
        <div className="form-grid">
          <Field label="股票刷新间隔（秒）">
            <input
              type="number"
              min={15}
              value={settings.stockSeconds}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  stockSeconds: Number(e.target.value),
                })
              }
            />
          </Field>
          <Field label="币种刷新间隔（秒）">
            <input
              type="number"
              min={30}
              value={settings.cryptoSeconds}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  cryptoSeconds: Number(e.target.value),
                })
              }
            />
          </Field>
          <Field label="黄金刷新间隔（秒）">
            <input
              type="number"
              min={30}
              value={settings.goldSeconds}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  goldSeconds: Number(e.target.value),
                })
              }
            />
          </Field>
          <Field label="CoinGecko 模式">
            <select
              value={settings.coinGeckoMode}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  coinGeckoMode: e.target
                    .value as typeof settings.coinGeckoMode,
                })
              }
            >
              <option value="public">免费公开参考源</option>
              <option value="demo">Demo Key</option>
              <option value="pro">Pro Key</option>
            </select>
          </Field>
          <Field label="CoinGecko 授权月请求上限">
            <input
              type="number"
              min={1}
              value={settings.coinGeckoMonthlyCap}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  coinGeckoMonthlyCap: Number(e.target.value),
                })
              }
            />
          </Field>
          <Field label="AllTick 日请求上限">
            <input
              type="number"
              min={1}
              value={settings.allTickDailyCap}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  allTickDailyCap: Number(e.target.value),
                })
              }
            />
          </Field>
        </div>
        <Field label="股票与主流币报价优先源">
          <select
            value={String(settings.allTickEnabled)}
            onChange={(e) =>
              setSettings({
                ...settings,
                allTickEnabled: e.target.value === "true",
              })
            }
          >
            <option value="false">免费参考源</option>
            <option value="true">优先 AllTick（失败后使用免费源）</option>
          </select>
        </Field>
        {settings.allTickEnabled && (
          <div className="form-grid">
            <Field label="AllTick 获取方式">
              <select
                value={settings.allTickTransport}
                onChange={(e) =>
                  setSettings({
                    ...settings,
                    allTickTransport: e.target.value as "polling" | "push",
                  })
                }
              >
                <option value="polling">按配置间隔批量轮询</option>
                <option value="push">逐笔推送（需要对应授权）</option>
              </select>
            </Field>
            <Field
              label="每类推送最大订阅数"
              hint="默认 5，按购买的实际权限设置；超出部分继续轮询，连接及重新订阅计入日请求上限，心跳控制消息不计入。"
            >
              <input
                type="number"
                min={1}
                max={3000}
                value={settings.allTickSubscriptionCap}
                onChange={(e) =>
                  setSettings({
                    ...settings,
                    allTickSubscriptionCap: Number(e.target.value),
                  })
                }
              />
            </Field>
          </div>
        )}
      </section>
      <section className="panel investment-source-settings">
        <h2>授权数据源</h2>
        <p className="muted">
          密钥在本机加密，保存在便携数据目录。换电脑后重新填写。行情、周期与频率按服务商实际权限提供，密钥不会交给网页。
        </p>
        {status.error && <p className="warning">{status.error}</p>}
        <div className="form-grid">
          <Field
            label={`AllTick Token · ${status.allTick ? "已配置" : "未配置"}`}
          >
            <input
              type="password"
              autoComplete="off"
              value={allTick}
              onChange={(e) => setAllTick(e.target.value)}
              placeholder="输入新 Token，留空保留原配置"
            />
          </Field>
          <Field
            label={`CoinGecko Demo / Pro Key · ${status.coinGecko ? "已配置" : "未配置"}`}
          >
            <input
              type="password"
              autoComplete="off"
              value={cgKey}
              onChange={(e) => setCgKey(e.target.value)}
              placeholder="输入新 Key，留空保留原配置"
            />
          </Field>
        </div>
        <div className="backup-actions">
          <Button
            variant="ghost"
            disabled={busy}
            onClick={() => {
              if (confirm("确认移除已保存的行情密钥？"))
                void service
                  .saveSecrets({ allTick: "", coinGecko: "" })
                  .then(setStatus)
                  .catch(service.notify);
            }}
          >
            清除已保存密钥
          </Button>
          <Button disabled={busy} onClick={() => void save()}>
            {busy ? "保存中…" : "保存行情设置"}
          </Button>
        </div>
        {message && (
          <p className="muted" role="status">
            {message}
          </p>
        )}
        <p className="market-notice">
          免费接口不承诺交易所级实时性。公开币价可能缓存约一分钟；授权权限不足、配额耗尽和断网都会显示状态并保留最后有效报价。
        </p>
      </section>
    </>
  );
}
