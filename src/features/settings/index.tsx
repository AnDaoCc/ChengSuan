import { investmentService } from "../investment/service";
import { useState } from "react";
import {
  Sun,
  Moon,
  Monitor,
  Database,
  FolderLock,
  ShieldCheck,
  HardDrive,
  Download,
  Upload,
} from "lucide-react";
import { useApp } from "../../app/store";
import { native } from "../../infrastructure/desktop";
import { Button } from "../../shared/ui/button";
import { Field, CurrencySelect } from "../../shared/ui/fields";
export function SettingsPage() {
  const { settings, dataDirectory, app } = useApp();
  const [busy, setBusy] = useState(false);
  const set = async (v: Partial<typeof settings>) => {
    try {
      await app.settings(v);
    } catch (e) {
      app.notify(e);
    }
  };
  const backup = async () => {
    setBusy(true);
    try {
      await app.flush();
      await investmentService.flush();
      app.notify(`一致性备份已保存：${await native.backup()}`);
    } catch (e) {
      app.notify(e);
    } finally {
      setBusy(false);
    }
  };
  const restore = async () => {
    setBusy(true);
    try {
      await app.flush();
      await investmentService.flush();
      if (await native.restore()) {
        await app.load();
        await investmentService.reload();
        app.notify("数据库已恢复");
      }
    } catch (e) {
      app.notify(e);
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <header className="page-header">
        <div>
          <div className="eyebrow">MAKE IT YOURS</div>
          <h1>应用设置</h1>
          <p>你的习惯，你的数据，你的澄算。</p>
        </div>
      </header>
      <section className="panel settings-panel">
        <h2>外观与计算</h2>
        <Field label="界面主题">
          <div className="theme-options">
            {(
              [
                { id: "light", text: "浅色", Icon: Sun },
                { id: "dark", text: "深色", Icon: Moon },
                { id: "system", text: "跟随系统", Icon: Monitor },
              ] as const
            ).map(({ id, text, Icon }) => (
              <button
                key={id}
                className={settings.theme === id ? "selected" : ""}
                onClick={() => void set({ theme: id })}
              >
                <Icon size={22} />
                {text}
              </button>
            ))}
          </div>
        </Field>
        <div className="form-grid">
          <Field label="新方案默认汇总币种" hint="已有方案的币种不会因此改变。">
            <CurrencySelect
              value={settings.defaultCurrency}
              onChange={(defaultCurrency) => void set({ defaultCurrency })}
            />
          </Field>
          <Field
            label="展示与导出金额精度"
            hint="仅影响格式化，存储和计算保留原始精度。"
          >
            <select
              value={settings.precision ?? "auto"}
              onChange={(e) =>
                void set({
                  precision:
                    e.target.value === "auto" ? null : Number(e.target.value),
                })
              }
            >
              <option value="auto">按币种标准精度</option>
              {Array.from({ length: 9 }, (_, i) => (
                <option key={i} value={i}>
                  {i} 位小数
                </option>
              ))}
            </select>
          </Field>
        </div>
      </section>
      <section className="panel settings-panel">
        <h2>
          <Database size={19} />
          数据与备份
        </h2>
        <div className="data-directory">
          <FolderLock size={24} />
          <div>
            <strong>便携数据目录</strong>
            <code>{dataDirectory || "EXE 所在目录 / ChengSuanData"}</code>
          </div>
        </div>
        <p className="muted">
          移动软件时，将 ChengSuan.exe 和 ChengSuanData
          一起移动。升级时先退出软件，再替换 EXE 并保留数据文件夹。
        </p>
        <div className="backup-actions">
          <Button
            variant="secondary"
            disabled={busy}
            onClick={() => void backup()}
          >
            <Download size={16} />
            创建安全备份
          </Button>
          <Button
            variant="secondary"
            disabled={busy}
            onClick={() => void restore()}
          >
            <Upload size={16} />
            从备份恢复
          </Button>
        </div>
        <small className="muted">
          恢复会预览方案、账户及交易数量并再次确认，恢复前自动备份当前数据库。
        </small>
      </section>
      <section className="panel about-panel">
        <img src="/brand.png" alt="澄算图标" />
        <div>
          <h2>
            澄算 ChengSuan <span className="badge">1.1.0</span>
          </h2>
          <p>个人成本计算与投资持仓监控，让每份投入都有依据。</p>
          <p className="muted">
            图表：TradingView Lightweight Charts™ · Copyright © 2025
            TradingView, Inc. · Apache 2.0
          </p>
          <div className="about-tags">
            <span>
              <HardDrive size={14} />
              Windows x64 便携版
            </span>
            <span>
              <ShieldCheck size={14} />
              本地保存，离线优先
            </span>
          </div>
        </div>
      </section>
    </>
  );
}
