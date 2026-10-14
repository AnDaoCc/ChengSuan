import { useEffect, useRef, useState } from "react";
import { Search, RefreshCw, Check, AlertCircle } from "lucide-react";
import { useInvestment } from "./hooks";
import {
  accountSchema,
  accountLabels,
  tradeSchema,
  instrumentSchema,
  sideLabels,
  type Instrument,
  type Trade,
  type InvestmentAccount,
} from "../../core/investment/model";
import { D } from "../../core/investment/engine";
import { Modal } from "../../shared/ui/dialog";
import { Field } from "../../shared/ui/fields";
import { Button } from "../../shared/ui/button";
import { errorText } from "../../shared/utils";
import { cash, price } from "./format";

function localDateTime(date = new Date()) {
  return date
    .toLocaleString("sv-SE", { hour12: false })
    .slice(0, 16)
    .replace(" ", "T");
}
export function AssetPicker({
  onSelect,
  onClose,
}: {
  onSelect: (asset: Instrument) => void;
  onClose: () => void;
}) {
  const { service, directoryVersion } = useInvestment();
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState("stocks");
  const [assets, setAssets] = useState<Instrument[]>([]);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(true);
  useEffect(() => {
    let cancelled = false;
    const timer = setTimeout(() => {
      setBusy(true);
      void service
        .search(category, query)
        .then((result) => {
          if (!cancelled) {
            setAssets(result.items);
            setError(
              result.busy && !result.items.length
                ? "股票与 ETF 目录正在更新，请稍候…"
                : "",
            );
          }
        })
        .catch((e) => {
          if (!cancelled) setError(errorText(e));
        })
        .finally(() => {
          if (!cancelled) setBusy(false);
        });
    }, 350);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [query, category, directoryVersion, service]);
  return (
    <Modal
      open
      onOpenChange={(v) => {
        if (!v) onClose();
      }}
      title="选择投资标的"
    >
      <div className="picker-search">
        <select value={category} onChange={(e) => setCategory(e.target.value)}>
          <option value="stocks">沪深京股票</option>
          <option value="etf">普通 ETF</option>
          <option value="goldEtf">黄金 ETF</option>
          <option value="gold">实物黄金</option>
          <option value="crypto">加密货币</option>
        </select>
        <input
          aria-label="搜索投资标的"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="名称、股票代码、BTC/USDT…"
        />
      </div>
      {error && <p className="muted">{error}</p>}
      {busy && <p className="muted">搜索中…</p>}
      <div className="asset-picker-list">
        {assets.map((asset) => (
          <button key={asset.id} onClick={() => onSelect(asset)}>
            <strong>{asset.name}</strong>
            <span>
              {asset.symbol} · {asset.market} · {asset.unit}
            </span>
          </button>
        ))}
      </div>
      <p className="muted">
        链上合约币请先在行情市场中“添加合约币”，然后从资产详情记录交易。
      </p>
    </Modal>
  );
}
export function TradeForm({
  asset,
  existing,
  initialSide = "buy",
  accountId,
  onClose,
}: {
  asset: Instrument;
  existing?: Trade;
  initialSide?: Trade["side"];
  accountId?: string;
  onClose: () => void;
}) {
  const { document, service } = useInvestment();
  const initial = useRef<Trade>(
    existing ?? {
      id: crypto.randomUUID(),
      accountId: accountId ?? document!.accounts[0].id,
      instrumentId: asset.id,
      side: initialSide,
      executedAt: new Date().toISOString(),
      sequence: Math.max(0, ...document!.trades.map((t) => t.sequence)) + 1,
      quantity: "",
      unitPrice: "",
      fee: "0",
      currency: asset.kind === "crypto" ? "USDT" : "CNY",
      fxToCny: asset.kind === "crypto" ? "" : "1",
      fxDate: new Date().toLocaleDateString("en-CA"),
      fxSource: asset.kind === "crypto" ? "手动成交换算" : "人民币",
      note: "",
    },
  );
  const [trade, setTrade] = useState<Trade>(initial.current);
  const latest = useRef(trade);
  latest.current = trade;
  const [date, setDate] = useState(localDateTime(new Date(trade.executedAt)));
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [fxBusy, setFxBusy] = useState(false);
  const dirty = JSON.stringify(trade) !== JSON.stringify(initial.current);
  const close = () => {
    if (dirty && !confirm("这笔交易尚未保存，确认放弃修改？")) return;
    onClose();
  };
  useEffect(() => {
    service.setDraft(trade.id, dirty);
    return () => service.setDraft(trade.id, false);
  }, [dirty, trade.id, service]);
  const update = (v: Partial<Trade>) => setTrade({ ...trade, ...v });
  const changeSide = (side: Trade["side"]) =>
    update(
      side === "adjust"
        ? {
            side,
            currency: "CNY",
            fxToCny: "1",
            fxSource: "手工持仓校正",
            fee: "0",
          }
        : { side },
    );
  const lookup = async () => {
    setFxBusy(true);
    try {
      const currency = trade.currency;
      const day = date.slice(0, 10);
      const result = await service.fx(currency, day);
      if (
        latest.current.currency === currency &&
        latest.current.executedAt === trade.executedAt
      )
        setTrade((current) => ({
          ...current,
          fxToCny: result.rate,
          fxDate: result.date,
          fxSource: result.source,
        }));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setFxBusy(false);
    }
  };
  const save = async () => {
    setBusy(true);
    try {
      const parsed = tradeSchema.parse(trade);
      await service.saveTrade(asset, parsed);
      onClose();
    } catch (e) {
      setError(
        e instanceof Error && "issues" in e
          ? (e as { issues: { message: string }[] }).issues
              .map((i) => i.message)
              .join("；")
          : errorText(e),
      );
    } finally {
      setBusy(false);
    }
  };
  let preview = "—";
  try {
    if (trade.quantity && trade.unitPrice && trade.fxToCny) {
      const amount = new D(trade.quantity).mul(trade.unitPrice);
      preview = cash(
        (trade.side === "sell"
          ? amount.minus(trade.fee)
          : amount.plus(trade.fee)
        )
          .mul(trade.fxToCny)
          .toFixed(),
      );
    }
  } catch {
    /* incomplete form */
  }
  return (
    <Modal
      open
      onOpenChange={(v) => {
        if (!v) close();
      }}
      title={`${existing ? "编辑" : "记录"}投资交易`}
      description="填写实际成交参数，点击保存后才会记入持仓。"
    >
      <div className="selected-asset">
        <strong>{asset.name}</strong>
        <span>
          {asset.symbol} · {asset.market} · 单位：{asset.unit}
        </span>
      </div>
      <div className="form-grid">
        <Field label="投资账户">
          <select
            value={trade.accountId}
            onChange={(e) => update({ accountId: e.target.value })}
          >
            {document!.accounts.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
          </select>
        </Field>
        <Field label="记录类型">
          <select
            value={trade.side}
            onChange={(e) => changeSide(e.target.value as Trade["side"])}
          >
            {Object.entries(sideLabels).map(([side, label]) => (
              <option key={side} value={side}>
                {label}
              </option>
            ))}
          </select>
        </Field>
        <Field label="实际成交时间">
          <input
            type="datetime-local"
            value={date}
            onChange={(e) => {
              setDate(e.target.value);
              const stamp = new Date(e.target.value);
              if (!Number.isNaN(stamp.valueOf()))
                update({
                  executedAt: stamp.toISOString(),
                  fxDate: e.target.value.slice(0, 10),
                  fxToCny: trade.currency === "CNY" ? "1" : "",
                  fxSource:
                    trade.currency === "CNY" ? "人民币" : "手动成交换算",
                });
            }}
          />
        </Field>
        <Field
          label={
            trade.side === "adjust"
              ? `校正后数量（${asset.unit}）`
              : `成交数量（${asset.unit}）`
          }
          hint="使用实际到账／成交数量，股票及 ETF 输入整数股／份。"
        >
          <input
            inputMode="decimal"
            value={trade.quantity}
            onChange={(e) => update({ quantity: e.target.value })}
          />
        </Field>
        <Field
          label={
            trade.side === "adjust" ? "校正后每单位成本（元）" : "实际成交单价"
          }
        >
          <input
            inputMode="decimal"
            value={trade.unitPrice}
            onChange={(e) => update({ unitPrice: e.target.value })}
          />
        </Field>
        <Field label="成交计价币种">
          <select
            value={trade.currency}
            disabled={trade.side === "adjust"}
            onChange={(e) =>
              update({
                currency: e.target.value as Trade["currency"],
                fxToCny: e.target.value === "CNY" ? "1" : "",
                fxSource: e.target.value === "CNY" ? "人民币" : "手动成交换算",
              })
            }
          >
            <option value="CNY">CNY · 人民币</option>
            <option value="USD">USD · 美元</option>
            <option value="USDT">USDT · 泰达币</option>
          </select>
        </Field>
        <Field
          label={`手续费（${trade.currency}）`}
          hint="其他币种手续费请换算后填写，币数以实际到账净数量为准。"
        >
          <input
            inputMode="decimal"
            disabled={trade.side === "adjust"}
            value={trade.fee}
            onChange={(e) => update({ fee: e.target.value })}
          />
        </Field>
        <Field
          label={`1 ${trade.currency} = 多少人民币`}
          hint={`参考日期：${trade.fxDate} · ${trade.fxSource}；保存后冻结`}
        >
          <div className="inline-field">
            <input
              inputMode="decimal"
              disabled={trade.currency === "CNY"}
              value={trade.fxToCny}
              onChange={(e) =>
                update({ fxToCny: e.target.value, fxSource: "手动成交换算" })
              }
            />
            {trade.currency !== "CNY" && (
              <button
                className="icon-button"
                aria-label="查询成交日期参考汇率"
                title="查询日期参考汇率，核对后可修改"
                disabled={fxBusy}
                onClick={() => void lookup()}
              >
                <RefreshCw size={17} className={fxBusy ? "spinning" : ""} />
              </button>
            )}
          </div>
        </Field>
        <Field label={trade.side === "adjust" ? "校正依据（必填）" : "备注"}>
          <textarea
            value={trade.note}
            onChange={(e) => update({ note: e.target.value })}
          />
        </Field>
      </div>
      <div className="trade-preview">
        <span>
          {trade.side === "sell"
            ? "卖出净额"
            : trade.side === "adjust"
              ? "校正后持仓成本"
              : "本笔投入成本"}{" "}
          · 人民币
        </span>
        <strong>{preview}</strong>
      </div>
      {error && (
        <div className="warning">
          <AlertCircle size={16} />
          {error}
        </div>
      )}
      <div className="modal-footer">
        <Button variant="secondary" onClick={close}>
          取消
        </Button>
        <Button disabled={busy || fxBusy} onClick={() => void save()}>
          <Check size={16} />
          {busy ? "保存中…" : "保存交易"}
        </Button>
      </div>
    </Modal>
  );
}
export function AccountForm({
  existing,
  onClose,
}: {
  existing?: InvestmentAccount;
  onClose: () => void;
}) {
  const { service } = useInvestment();
  const [account, setAccount] = useState(
    existing ?? {
      id: crypto.randomUUID(),
      name: "",
      kind: "general" as const,
      note: "",
    },
  );
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const save = async () => {
    setBusy(true);
    try {
      await service.saveAccount(accountSchema.parse(account));
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
      onOpenChange={(v) => {
        if (!v) onClose();
      }}
      title={existing ? "编辑账户" : "新增投资账户"}
      description="账户用于分类记录持仓，不会连接证券或交易所交易权限。"
    >
      <Field label="账户名称">
        <input
          value={account.name}
          onChange={(e) => setAccount({ ...account, name: e.target.value })}
        />
      </Field>
      <Field label="账户类型">
        <select
          value={account.kind}
          onChange={(e) =>
            setAccount({
              ...account,
              kind: e.target.value as InvestmentAccount["kind"],
            })
          }
        >
          {Object.entries(accountLabels).map(([kind, label]) => (
            <option key={kind} value={kind}>
              {label}
            </option>
          ))}
        </select>
      </Field>
      <Field label="备注">
        <textarea
          value={account.note}
          onChange={(e) => setAccount({ ...account, note: e.target.value })}
        />
      </Field>
      {error && <p className="field-error">{error}</p>}
      <div className="modal-footer">
        <Button disabled={busy} onClick={() => void save()}>
          保存账户
        </Button>
      </div>
    </Modal>
  );
}
export function AssetSettings({
  asset,
  onClose,
}: {
  asset: Instrument;
  onClose: () => void;
}) {
  const { service } = useInvestment();
  const [draft, setDraft] = useState(asset);
  const [error, setError] = useState("");
  const save = async () => {
    try {
      await service.updateInstrument(instrumentSchema.parse(draft));
      onClose();
    } catch (e) {
      setError(errorText(e));
    }
  };
  return (
    <Modal
      open
      onOpenChange={(v) => {
        if (!v) onClose();
      }}
      title="资产估值设置"
      description="明确估值单位及来源，手动参考价会标记为估算。"
    >
      <Field label="显示名称">
        <input
          value={draft.name}
          onChange={(e) => setDraft({ ...draft, name: e.target.value })}
        />
      </Field>
      {asset.kind === "gold" && (
        <Field
          label="黄金纯度（0–1）"
          hint="Au99.99 默认为 0.9999；估值按纯度比例调整，不含回购价差。"
        >
          <input
            inputMode="decimal"
            value={draft.purity}
            onChange={(e) => setDraft({ ...draft, purity: e.target.value })}
          />
        </Field>
      )}
      <Field
        label={
          asset.kind === "gold"
            ? "手动 Au99.99 参考金价（元/克）"
            : "手动人民币参考单价"
        }
        hint="留空使用联网行情；手动估价不触发实时阈值提醒。"
      >
        <input
          inputMode="decimal"
          value={draft.manualPrice ?? ""}
          onChange={(e) =>
            setDraft({ ...draft, manualPrice: e.target.value || null })
          }
        />
      </Field>
      {asset.pools.length > 0 && (
        <Field
          label="估值交易池"
          hint="不同池报价可能不同，标记所选池与目标币所在方向。"
        >
          <select
            value={draft.pool?.address ?? ""}
            onChange={(e) =>
              setDraft({
                ...draft,
                pool: draft.pools.find((p) => p.address === e.target.value),
              })
            }
          >
            {draft.pools.map((p) => (
              <option key={p.address} value={p.address}>
                {p.name} · 流动性 ${price(p.liquidity)} · {p.tokenSide}
              </option>
            ))}
          </select>
        </Field>
      )}
      {error && <p className="field-error">{error}</p>}
      <div className="modal-footer">
        <Button onClick={() => void save()}>保存估值设置</Button>
      </div>
    </Modal>
  );
}
