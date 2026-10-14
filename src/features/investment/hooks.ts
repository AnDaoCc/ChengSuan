import { useSyncExternalStore } from "react";
import { investmentService } from "./service";
export function useInvestment() {
  return {
    ...useSyncExternalStore(
      investmentService.subscribe,
      investmentService.snapshot,
    ),
    service: investmentService,
  };
}
