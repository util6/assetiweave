interface CacheEntry<T> {
  data?: T;
  promise?: Promise<T>;
  updatedAt: number;
}

const resourceCache = new Map<string, CacheEntry<unknown>>();

export function readSharedResource<T>(key: string) {
  return (resourceCache.get(key) as CacheEntry<T> | undefined)?.data;
}

export function loadSharedResource<T>(
  key: string,
  loader: () => Promise<T>,
  options: { force?: boolean } = {},
) {
  const current = resourceCache.get(key) as CacheEntry<T> | undefined;
  if (current?.promise) {
    return current.promise;
  }
  if (current?.data !== undefined && !options.force) {
    return Promise.resolve(current.data);
  }

  const entry: CacheEntry<T> = current ?? { updatedAt: 0 };
  const request = Promise.resolve()
    .then(loader)
    .then((data) => {
      entry.data = data;
      entry.updatedAt = Date.now();
      if (entry.promise === request) {
        entry.promise = undefined;
      }
      return data;
    })
    .catch((error) => {
      if (entry.promise === request) {
        entry.promise = undefined;
      }
      throw error;
    });
  entry.promise = request;
  resourceCache.set(key, entry);
  return request;
}

export function writeSharedResource<T>(key: string, data: T) {
  resourceCache.set(key, { data, updatedAt: Date.now() });
}

export function invalidateSharedResource(key: string) {
  const current = resourceCache.get(key) as CacheEntry<unknown> | undefined;
  if (!current) {
    return;
  }
  current.data = undefined;
  current.updatedAt = 0;
  if (!current.promise) {
    resourceCache.delete(key);
  }
}

export function clearSharedResourceCache() {
  resourceCache.clear();
}
