# Remember 前端对接手册（桌面壳层优先）

本手册面向前端工程师，目标是在不了解后端源码细节的前提下，快速完成前端与 Remember 后端的稳定对接。

## 1. 系统边界与运行模式

- 后端固定运行模式：`sqlite_only`
- 对外入口固定为本地 IPC（不提供 HTTP API）
- 生产主通道：Named Pipe（默认 `\\.\pipe\remember-ipc-v1`）
- 诊断兜底通道：Loopback（默认关闭，仅开发排障）
- 鉴权：每次请求都必须带 `authToken`

前端团队需要明确：

- 前端不能直接访问 SQLite
- 前端不能假设存在远程 API
- 前端不能依赖历史字段（如 `runtimeMode`、`usedFallback`、`startupSelfHeal`）

## 2. 快速接入流程（最小闭环）

### 2.1 启动后端

```powershell
$env:REMEMBER_IPC_AUTH_TOKEN = "remember-local-dev-token"
cargo run -p remember-ipc-server
```

期望日志包含：

- `ipc server started`

### 2.2 健康探测

```powershell
$env:REMEMBER_IPC_AUTH_TOKEN = "remember-local-dev-token"
cargo run -p remember-cli -- health
```

期望输出：

- `healthy`

### 2.3 首次业务调用（series.list）

```powershell
$payload = @{ query=""; includeArchived=$false; cursor=$null; limit=20 } | ConvertTo-Json -Compress
cargo run -p remember-cli -- rpc call --path series.list --payload $payload
```

期望响应：`ok=true`，`data.items` 为数组。

## 3. 传输与连接策略（推荐）

### 3.1 策略总览

- 默认：Named Pipe（生产路径）
- 兜底：Loopback（仅开发排障）
- 不建议在生产中默认切到 Loopback

### 3.2 前端连接状态模型

- `loading`：应用启动，尚未探测可用性
- `ready`：最近一次探测/业务调用成功
- `degraded`：短时间出现可恢复错误（如偶发 `os error 233`）
- `disconnected`：持续失败，需提示用户或触发重连

推荐状态切换条件：

- 单次失败：`ready -> degraded`
- 连续失败达到阈值（例如 3 次）：`degraded -> disconnected`
- 任意成功调用：恢复到 `ready`

### 3.3 重试与退避（建议）

- 对 `health` 和幂等读取（如 `series.list`、`timeline.list`）可自动重试
- 建议退避：`200ms -> 500ms -> 1000ms`，最大 3 次
- 非幂等调用（如 `commit.append`）失败时默认不自动重放，避免重复写入

## 4. IPC v1 协议

### 4.1 请求格式

```json
{
  "id": "string",
  "path": "series.list",
  "payload": {},
  "authToken": "string"
}
```

约束：

- 字段命名使用 `camelCase`
- `id` 建议用 UUID
- `path` 必须是受支持的 RPC 路径

### 4.2 响应 Envelope

```json
{
  "ok": true,
  "data": {},
  "error": null,
  "meta": {
    "requestId": "string",
    "path": "series.list",
    "transport": "named_pipe",
    "respondedAtUnixMs": 0
  }
}
```

语义：

- `ok=true` 时读取 `data`
- `ok=false` 时读取 `error`
- `meta` 可用于日志关联和诊断

## 5. RPC 契约（6 条）

### 5.1 series.create

- Path：`series.create`
- Request：`{ "name": string }`
- 必填约束：`name` 非空字符串
- Success：`{ "series": SeriesSummary }`
- 常见失败码：`VALIDATION_ERROR`

### 5.2 series.list

- Path：`series.list`
- Request：`{ "query": string, "includeArchived": boolean, "cursor": string | null, "limit": number }`
- 必填约束：
  - `query` 必须存在且为字符串
  - `includeArchived` 必须存在且为布尔值
  - `cursor` 必须存在，且为字符串或 null
  - `limit` 必须为正整数（>0）
- Success：`{ "items": SeriesSummary[], "nextCursor": string | null, "limitEcho": number }`
- 常见失败码：`VALIDATION_ERROR`

### 5.3 commit.append

- Path：`commit.append`
- Request：`{ "seriesId": string, "content": string, "clientTs": string }`
- 必填约束：
  - `seriesId` 非空字符串
  - `content` 非空字符串
  - `clientTs` 非空且应为 RFC3339 时间字符串
- Success：`{ "commit": CommitItem, "series": SeriesSummary }`
- 常见失败码：`VALIDATION_ERROR`、`NOT_FOUND`、`CONFLICT`

### 5.4 timeline.list

- Path：`timeline.list`
- Request：`{ "seriesId": string, "cursor": string | null, "limit": number }`
- 必填约束：
  - `seriesId` 非空字符串
  - `cursor` 必须存在，且为字符串或 null
  - `limit` 必须为正整数（>0）
- Success：`{ "seriesId": string, "items": CommitItem[], "nextCursor": string | null }`
- 常见失败码：`VALIDATION_ERROR`、`NOT_FOUND`

### 5.5 series.archive

- Path：`series.archive`
- Request：`{ "seriesId": string }`
- 必填约束：`seriesId` 非空字符串
- Success：`{ "seriesId": string, "archivedAt": string }`
- 常见失败码：`VALIDATION_ERROR`、`NOT_FOUND`

### 5.6 series.scan_silent

- Path：`series.scan_silent`
- Request：`{ "now": string, "thresholdDays": number }`
- 必填约束：
  - `now` 非空字符串且应为 RFC3339
  - `thresholdDays` 为非负整数（可为 0）
- Success：`{ "affectedSeriesIds": string[], "thresholdDays": number }`
- 常见失败码：`VALIDATION_ERROR`

## 6. 类型定义（前端可直接使用）

```ts
export type RpcErrorCode =
  | "VALIDATION_ERROR"
  | "NOT_FOUND"
  | "CONFLICT"
  | "UNKNOWN_COMMAND"
  | "INTERNAL_ERROR";

export interface RpcError {
  code: RpcErrorCode;
  message: string;
}

export interface RpcMeta {
  requestId: string;
  path: string;
  transport: "named_pipe" | "loopback" | string;
  respondedAtUnixMs: number;
}

export interface RpcEnvelope<T> {
  ok: boolean;
  data?: T;
  error?: RpcError;
  meta: RpcMeta;
}

export type SeriesStatus = "active" | "silent" | "archived";

export interface SeriesSummary {
  id: string;
  name: string;
  status: SeriesStatus;
  lastUpdatedAt: string;
  latestExcerpt: string;
  createdAt: string;
  archivedAt?: string;
}

export interface CommitItem {
  id: string;
  seriesId: string;
  content: string;
  createdAt: string;
}

export interface SeriesListData {
  items: SeriesSummary[];
  nextCursor: string | null;
  limitEcho: number;
}

export interface TimelineListData {
  seriesId: string;
  items: CommitItem[];
  nextCursor: string | null;
}

export interface SeriesScanSilentData {
  affectedSeriesIds: string[];
  thresholdDays: number;
}
```

## 7. 可复用 TS 客户端模板

```ts
type RpcPath =
  | "series.create"
  | "series.list"
  | "commit.append"
  | "timeline.list"
  | "series.archive"
  | "series.scan_silent";

interface IpcRequest {
  id: string;
  path: RpcPath;
  payload: unknown;
  authToken: string;
}

interface TransportAdapter {
  // 输入一行 JSON，请求返回一行 JSON
  send(requestLine: string, opts?: { transport?: "named_pipe" | "loopback" }): Promise<string>;
}

export class RememberRpcException extends Error {
  constructor(
    public code: string,
    message: string,
    public meta?: RpcMeta
  ) {
    super(message);
  }
}

export class RememberClient {
  constructor(
    private readonly adapter: TransportAdapter,
    private readonly authToken: string
  ) {}

  private async invokeRpc<T>(
    path: RpcPath,
    payload: unknown,
    opts?: { transport?: "named_pipe" | "loopback" }
  ): Promise<T> {
    const request: IpcRequest = {
      id: crypto.randomUUID(),
      path,
      payload,
      authToken: this.authToken
    };

    const responseLine = await this.adapter.send(JSON.stringify(request), opts);
    const envelope = JSON.parse(responseLine) as RpcEnvelope<T>;

    if (!envelope.ok) {
      const err = envelope.error ?? { code: "INTERNAL_ERROR", message: "unknown rpc error" };
      throw new RememberRpcException(err.code, err.message, envelope.meta);
    }

    return envelope.data as T;
  }

  createSeries(name: string) {
    return this.invokeRpc<{ series: SeriesSummary }>("series.create", { name });
  }

  listSeries(input: { query: string; includeArchived: boolean; cursor: string | null; limit: number }) {
    return this.invokeRpc<SeriesListData>("series.list", input);
  }

  appendCommit(input: { seriesId: string; content: string; clientTs: string }) {
    return this.invokeRpc<{ commit: CommitItem; series: SeriesSummary }>("commit.append", input);
  }

  listTimeline(input: { seriesId: string; cursor: string | null; limit: number }) {
    return this.invokeRpc<TimelineListData>("timeline.list", input);
  }

  archiveSeries(seriesId: string) {
    return this.invokeRpc<{ seriesId: string; archivedAt: string }>("series.archive", { seriesId });
  }

  scanSilent(input: { now: string; thresholdDays: number }) {
    return this.invokeRpc<SeriesScanSilentData>("series.scan_silent", input);
  }
}
```

### 7.1 桌面壳层适配建议

无论你使用 Electron、WinUI WebView2、Tauri 或其他壳层，都建议做三层隔离：

- UI 层：React/Vue/Solid 等，仅依赖 `RememberClient`
- Bridge 层：把 UI 请求转发给本地 IPC（可做权限与审计）
- IPC 层：Named Pipe / Loopback 通道实现

注意：

- 本后端不承诺 `invoke("rpc_invoke")` 兼容接口
- 若使用 Tauri/Electron，请自行在桥接层实现 IPC v1 request/response 转换

## 8. 错误处理与 UI 映射

| 错误码 | 建议 UI 行为 |
|---|---|
| `VALIDATION_ERROR` | 显示表单错误或参数错误提示，可直接修正后重试 |
| `NOT_FOUND` | 提示资源不存在，建议刷新列表或回到上一层 |
| `CONFLICT` | 提示冲突（如归档后不可追加），阻断当前操作 |
| `UNKNOWN_COMMAND` | 视为前后端版本不匹配，提示升级 |
| `INTERNAL_ERROR` | 显示通用错误，记录 `meta.requestId` 并引导重试 |

## 9. 联调与验收清单

### 9.1 本地联调命令

```powershell
cargo check --workspace
cargo test --workspace -- --nocapture
powershell -ExecutionPolicy Bypass -File scripts/smoke-ipc.ps1
powershell -ExecutionPolicy Bypass -File scripts/regression-b6-t1.ps1
```

### 9.2 前端接入验收标准

- 能稳定完成：`health -> series.list -> series.create -> commit.append -> timeline.list`
- 能正确处理 `CONFLICT`（归档后追加 commit）
- UI 层已实现 `degraded/disconnected` 状态和自动恢复
- 生产默认走 Named Pipe，不将 Loopback 作为默认主路径

## 10. 常见问题定位

### 10.1 `os error 233`（管道另一端无任何进程）

含义：连接后读取响应失败，通常是服务端连接生命周期抖动或瞬时断开。  
建议：

1. 先做一次短退避重试（最多 3 次）
2. 记录 `requestId` 与时间戳
3. 若持续失败，提示用户重启后端进程
4. 在开发态可切 Loopback 验证是否为 Named Pipe 层问题

### 10.2 `invalid payload json`

含义：传输层传入的 payload 不是合法 JSON。  
建议：统一在桥接层 `JSON.stringify` 后再发送，不在 UI 层拼接字符串。

### 10.3 `invalid auth token`

含义：请求中的 token 与服务端不一致。  
建议：前端启动时读取同一份 token 配置源，并在连接失败日志中输出 token 来源（不要输出 token 明文）。

## 11. 不承诺项与兼容红线

- 不承诺 HTTP API
- 不承诺多数据库运行时切换
- 不承诺历史 Postgres/双写链路
- 不承诺 Tauri `invoke("rpc_invoke")` 兼容
- 前端不要依赖非契约字段或历史遗留字段

