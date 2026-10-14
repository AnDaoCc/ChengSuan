import { investmentRepository } from "../../infrastructure/investment";
import {
  documentSchema,
  type InvestmentDocument,
  type Instrument,
  type Trade,
  type InvestmentAccount,
  type AlertState,
  type AlertEvent,
  type MarketQuote,
} from "../../core/investment/model";
import { evaluateAlerts, portfolio } from "../../core/investment/engine";
import { errorText } from "../../shared/utils";
import { desktop } from "../../infrastructure/desktop";

type Snapshot = {
  document: InvestmentDocument | null;
  quotes: MarketQuote[];
  errors: string[];
  loading: boolean;
  saving: boolean;
  error: string;
  message: string;
  events: AlertEvent[];
  directoryBusy: boolean;
  directoryMessage: string;
  directoryVersion: number;
};
export class InvestmentService {
  private state: Snapshot = {
    document: null,
    quotes: [],
    errors: [],
    loading: true,
    saving: false,
    error: "",
    message: "",
    events: [],
    directoryBusy: false,
    directoryMessage: "",
    directoryVersion: 0,
  };
  private listeners = new Set<() => void>();
  private chain: Promise<unknown> = Promise.resolve();
  private pendingWrites = 0;
  private started = false;
  private alertStates: Record<string, AlertState> = {};
  private alertChain: Promise<unknown> = Promise.resolve();
  private dirtyDrafts = new Set<string>();
  setDraft(id: string, dirty: boolean) {
    if (dirty) this.dirtyDrafts.add(id);
    else this.dirtyDrafts.delete(id);
  }
  hasDrafts() {
    return this.dirtyDrafts.size > 0;
  }
  subscribe = (callback: () => void) => {
    this.listeners.add(callback);
    return () => {
      this.listeners.delete(callback);
    };
  };
  snapshot = () => this.state;
  private patch(v: Partial<Snapshot>) {
    this.state = { ...this.state, ...v };
    this.listeners.forEach((callback) => callback());
  }
  notify = (error: unknown) => this.patch({ message: errorText(error) });
  dismiss = () => this.patch({ message: "" });
  async initialize() {
    if (this.started) return;
    this.started = true;
    await this.reload();
    if (!desktop) return;
    try {
      await investmentRepository.quoteListener((data) => {
        this.patch({ quotes: data.quotes, errors: data.errors });
        this.alertChain = this.alertChain
          .catch(() => {})
          .then(() => this.evaluate())
          .catch(this.notify);
      });
      await investmentRepository.directoryListener((data) =>
        this.patch({
          directoryBusy: data.busy,
          directoryMessage: data.message,
          directoryVersion: this.state.directoryVersion + (data.busy ? 0 : 1),
        }),
      );
      await investmentRepository.restoreListener(() => void this.reload());
      const data = await investmentRepository.snapshot();
      this.patch({ quotes: data.quotes, errors: data.errors });
      this.queueEvaluation();
    } catch (error) {
      this.notify(error);
    }
  }
  private queueEvaluation() {
    this.alertChain = this.alertChain
      .catch(() => {})
      .then(() => this.evaluate())
      .catch(this.notify);
  }
  async reload() {
    try {
      const data = await investmentRepository.load();
      portfolio(data.document);
      this.alertStates = Object.fromEntries(
        data.alertStates.map((row) => [row.id, row.state]),
      );
      this.patch({
        document: data.document,
        events: data.events,
        loading: false,
        error: "",
      });
    } catch (error) {
      this.patch({ loading: false, error: errorText(error) });
    }
  }
  edit(
    transform: (doc: InvestmentDocument) => InvestmentDocument | void,
  ): Promise<void> {
    this.pendingWrites++;
    this.patch({ saving: true });
    const write = this.chain
      .catch(() => {})
      .then(async () => {
        if (!this.state.document) throw Error("投资数据尚未加载");
        const previous = this.state.document;
        const draft = structuredClone(previous);
        const candidate = documentSchema.parse(transform(draft) ?? draft);
        portfolio(candidate);
        const revision = await investmentRepository.save(candidate);
        for (const id of Object.keys(this.alertStates)) {
          if (
            JSON.stringify(previous.alerts.find((r) => r.id === id)) !==
            JSON.stringify(candidate.alerts.find((r) => r.id === id))
          )
            delete this.alertStates[id];
        }
        this.patch({ document: { ...candidate, revision } });
        await investmentRepository.refresh();
      });
    this.chain = write.catch(() => {});
    return write.then(
      () => {
        this.pendingWrites--;
        this.patch({ saving: this.pendingWrites > 0 });
        this.queueEvaluation();
      },
      (error) => {
        this.pendingWrites--;
        this.patch({
          saving: this.pendingWrites > 0,
          message: errorText(error),
        });
        throw error;
      },
    );
  }
  async flush() {
    await this.chain;
    await this.alertChain;
  }
  private async evaluate() {
    if (!this.state.document) return;
    const result = evaluateAlerts(
      this.state.document,
      this.state.quotes,
      this.alertStates,
    );
    const changed =
      JSON.stringify(result.states) !== JSON.stringify(this.alertStates);
    if (!changed && !result.events.length) return;
    await investmentRepository.reportAlerts(result.states, result.events);
    this.alertStates = result.states;
    if (result.events.length)
      this.patch({
        events: [...result.events, ...this.state.events].slice(0, 200),
        message: result.events.map((e) => e.message).join("；"),
      });
  }
  saveTrade(asset: Instrument, trade: Trade) {
    return this.edit((doc) => {
      if (!doc.instruments.some((a) => a.id === asset.id))
        doc.instruments.push(asset);
      doc.trades = [...doc.trades.filter((t) => t.id !== trade.id), trade];
    });
  }
  deleteTrade(id: string) {
    return this.edit((doc) => {
      doc.trades = doc.trades.filter((t) => t.id !== id);
    });
  }
  watch(asset: Instrument) {
    return this.edit((doc) => {
      if (!doc.instruments.some((a) => a.id === asset.id))
        doc.instruments.push(asset);
      doc.watchlist = doc.watchlist.includes(asset.id)
        ? doc.watchlist.filter((id) => id !== asset.id)
        : [...doc.watchlist, asset.id];
    });
  }
  saveAccount(account: InvestmentAccount) {
    return this.edit((doc) => {
      doc.accounts = [
        ...doc.accounts.filter((a) => a.id !== account.id),
        account,
      ];
    });
  }
  updateInstrument(asset: Instrument) {
    const old = this.state.document?.instruments.find((a) => a.id === asset.id);
    return this.edit((doc) => {
      doc.instruments = [
        ...doc.instruments.filter((a) => a.id !== asset.id),
        asset,
      ];
    }).then(() => {
      if (old?.pool?.address !== asset.pool?.address)
        this.patch({
          quotes: this.state.quotes.filter((q) => q.instrumentId !== asset.id),
        });
    });
  }
  search = investmentRepository.search;
  visible = investmentRepository.visible;
  resolve = investmentRepository.resolve;
  fx = investmentRepository.fx;
  candles = investmentRepository.candles;
  secrets = investmentRepository.secretStatus;
  saveSecrets = investmentRepository.secretSave;
  refresh = investmentRepository.refresh;
}
export const investmentService = new InvestmentService();
