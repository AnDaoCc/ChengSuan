import { beforeEach, expect, it, vi } from "vitest";
import {
  defaultMonitorSettings,
  type InvestmentDocument,
} from "../../core/investment/model";
const repository = vi.hoisted(() => ({
  load: vi.fn(),
  save: vi.fn(),
  refresh: vi.fn(),
  snapshot: vi.fn(),
  quoteListener: vi.fn(),
  directoryListener: vi.fn(),
  restoreListener: vi.fn(),
  reportAlerts: vi.fn(),
  search: vi.fn(),
  visible: vi.fn(),
  resolve: vi.fn(),
  fx: vi.fn(),
  candles: vi.fn(),
  secretStatus: vi.fn(),
  secretSave: vi.fn(),
}));
vi.mock("../../infrastructure/investment", () => ({
  investmentRepository: repository,
}));
vi.mock("../../infrastructure/desktop", () => ({ desktop: true }));
import { InvestmentService } from "./service";
function doc(): InvestmentDocument {
  return {
    version: 1,
    revision: 0,
    accounts: [
      {
        id: "93f09c42-6a53-4c23-92b4-c24b264168aa",
        name: "我的投资",
        kind: "general",
        note: "",
      },
    ],
    instruments: [],
    trades: [],
    watchlist: [],
    alerts: [],
    settings: defaultMonitorSettings,
  };
}
beforeEach(() => {
  vi.resetAllMocks();
  repository.load.mockResolvedValue({
    document: doc(),
    alertStates: [],
    events: [],
  });
  repository.snapshot.mockResolvedValue({ quotes: [], errors: [] });
  repository.save.mockImplementation(async (d) => d.revision + 1);
});
it("rejecting an invalid edit preserves records and allows flush/navigation and later writes", async () => {
  const service = new InvestmentService();
  await service.initialize();
  await expect(
    service.edit((d) => {
      d.accounts = [];
    }),
  ).rejects.toThrow();
  await expect(service.flush()).resolves.toBeUndefined();
  expect(service.snapshot().document!.accounts).toHaveLength(1);
  await service.edit((d) => {
    d.accounts[0].name = "已修正的账户";
  });
  expect(service.snapshot().document!.accounts[0].name).toBe("已修正的账户");
  expect(repository.save).toHaveBeenCalledTimes(1);
});
it("serializes concurrent edits against the latest committed revision without lost updates", async () => {
  const service = new InvestmentService();
  await service.initialize();
  repository.save.mockImplementation(async (d) => {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return d.revision + 1;
  });
  const first = service.edit((d) => {
    d.accounts[0].name = "新账户名称";
  });
  const second = service.edit((d) => {
    d.accounts[0].note = "后续备注";
  });
  await first;
  expect(service.snapshot().saving).toBe(true);
  await second;
  await service.flush();
  expect(service.snapshot().document).toMatchObject({
    revision: 2,
    accounts: [{ name: "新账户名称", note: "后续备注" }],
  });
  expect(repository.save.mock.calls.map((call) => call[0].revision)).toEqual([
    0, 1,
  ]);
});
it("failed native commit does not replace the valid in-memory ledger", async () => {
  const service = new InvestmentService();
  await service.initialize();
  repository.save.mockRejectedValueOnce(Error("模拟磁盘写入失败"));
  await expect(
    service.edit((d) => {
      d.accounts[0].name = "不应显示为已保存";
    }),
  ).rejects.toThrow("磁盘");
  expect(service.snapshot().document!.accounts[0].name).toBe("我的投资");
  await expect(service.flush()).resolves.toBeUndefined();
});
