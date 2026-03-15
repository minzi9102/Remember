import type { LayerState } from "../application/types";

export interface AdapterSnapshot {
  adapter: LayerState;
  repository: LayerState;
}

export function readAdapterSnapshot(): AdapterSnapshot {
  return {
    adapter: "ready",
    repository: "stubbed",
  };
}
