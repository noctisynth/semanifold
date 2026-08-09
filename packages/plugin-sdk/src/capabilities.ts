export interface PluginHostV1 {
  listFiles(pattern: string): Promise<readonly string[]>;
  readText(path: string): Promise<string>;
}

export type PluginFetchHeadersInitV1 =
  | Readonly<Record<string, string>>
  | readonly (readonly [string, string])[];

export interface PluginFetchRequestInitV1 {
  readonly body?: string;
  readonly headers?: PluginFetchHeadersInitV1;
  readonly method?: string;
}

export interface PluginFetchHeadersV1 {
  append(name: string, value: string): void;
  delete(name: string): void;
  entries(): readonly (readonly [string, string])[];
  forEach(
    callback: (
      value: string,
      key: string,
      headers: PluginFetchHeadersV1,
    ) => void,
    thisArg?: unknown,
  ): void;
  get(name: string): string | null;
  getSetCookie(): readonly string[];
  has(name: string): boolean;
  keys(): readonly string[];
  set(name: string, value: string): void;
  values(): readonly string[];
}

export type PluginFetchResponseTypeV1 =
  | 'basic'
  | 'cors'
  | 'error'
  | 'opaque'
  | 'opaqueredirect';

export interface PluginFetchResponseV1 {
  readonly headers: PluginFetchHeadersV1;
  readonly status: number;
  readonly statusText: string;
  readonly type: PluginFetchResponseTypeV1;
  readonly url: string;
  bytes(): Promise<Uint8Array>;
  json(): Promise<unknown>;
  text(): Promise<string>;
}

export type PluginFetchV1 = (
  resource: string,
  options?: PluginFetchRequestInitV1,
) => Promise<PluginFetchResponseV1>;

export interface PluginUrlV1 {
  hash: string;
  host: string;
  hostname: string;
  href: string;
  readonly origin: string;
  password: string;
  pathname: string;
  port: string;
  protocol: string;
  search: string;
  username: string;
  toJSON(): string;
  toString(): string;
}

export interface PluginUrlConstructorV1 {
  new (url: string, base?: string): PluginUrlV1;
  canParse(url: string, base?: string): boolean;
  parse(url: string, base?: string): PluginUrlV1 | null;
}

declare global {
  const fetch: PluginFetchV1;
  const URL: PluginUrlConstructorV1;
}
