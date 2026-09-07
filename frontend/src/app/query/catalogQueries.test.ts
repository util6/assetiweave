import { QueryClient } from "@tanstack/react-query";
import { afterEach, expect, it, vi } from "vitest";
import { assetsQueryOptions, catalogKeys } from "./catalogQueries";

const listAssets = vi.hoisted(() => vi.fn(async () => []));
vi.mock("../../services/catalog", () => ({ listAssets }));

afterEach(() => vi.clearAllMocks());

it("同 scope 同 key 的并发 Catalog 读取只有一个请求", async () => {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const options = assetsQueryOptions(
    { tenantId: "workspace-a", epoch: 1 },
    "skill",
  );
  await Promise.all([client.fetchQuery(options), client.fetchQuery(options)]);
  expect(listAssets).toHaveBeenCalledTimes(1);
  expect(client.getQueryData(options.queryKey)).toEqual([]);
  client.clear();
});

it("catalogKeys 构造准确的多级资源键", () => {
  const scope = { tenantId: "tenant-1", epoch: 2 };
  expect(catalogKeys.root(scope)).toEqual(["tenant", "tenant-1", 2, "catalog"]);
  expect(catalogKeys.assets(scope, "skill")).toEqual([
    "tenant",
    "tenant-1",
    2,
    "catalog",
    "assets",
    "skill",
  ]);
  expect(catalogKeys.assets(scope)).toEqual([
    "tenant",
    "tenant-1",
    2,
    "catalog",
    "assets",
    "all",
  ]);
  expect(catalogKeys.sources(scope)).toEqual([
    "tenant",
    "tenant-1",
    2,
    "catalog",
    "sources",
  ]);
  expect(catalogKeys.profiles(scope)).toEqual([
    "tenant",
    "tenant-1",
    2,
    "catalog",
    "profiles",
  ]);
  expect(catalogKeys.overview(scope)).toEqual([
    "tenant",
    "tenant-1",
    2,
    "catalog",
    "overview",
  ]);
  expect(catalogKeys.shortcuts(scope)).toEqual([
    "tenant",
    "tenant-1",
    2,
    "catalog",
    "shortcuts",
  ]);
  expect(catalogKeys.mountStatuses(scope)).toEqual([
    "tenant",
    "tenant-1",
    2,
    "catalog",
    "mountStatuses",
  ]);
  expect(catalogKeys.navigation(scope)).toEqual([
    "tenant",
    "tenant-1",
    2,
    "catalog",
    "navigation",
  ]);
});
