import {
  LayoutGrid,
  Download,
  Settings,
  ChartCandlestick,
  type LucideIcon,
} from "lucide-react";
import type { ComponentType } from "react";
import { PlansPage } from "../features/plans";
import { ExportPage } from "../features/exports/page";
import { SettingsPage } from "../features/settings";
import { InvestmentPage } from "../features/investment";
export interface FeatureModule {
  id: string;
  label: string;
  route: string;
  Icon: LucideIcon;
  Page: ComponentType;
}
export const modules: readonly FeatureModule[] = [
  {
    id: "plans",
    label: "成本方案",
    route: "/",
    Icon: LayoutGrid,
    Page: PlansPage,
  },
  {
    id: "investment",
    label: "投资与持仓",
    route: "/investment",
    Icon: ChartCandlestick,
    Page: InvestmentPage,
  },
  {
    id: "exports",
    label: "导出与导入",
    route: "/export",
    Icon: Download,
    Page: ExportPage,
  },
  {
    id: "settings",
    label: "应用设置",
    route: "/settings",
    Icon: Settings,
    Page: SettingsPage,
  },
];
