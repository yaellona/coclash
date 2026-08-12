# mihomo RESTful API 完整列表

> Base URL: `http://127.0.0.1:9090`
> 鉴权头（配置了 secret 时）：`Authorization: Bearer <secret>`

| 分类　　　   | 方法　　 | 路径　　　　　　　　　　　　　　　　　　　　　　　　　　　　　 | 说明　　　　　　　　　　　　　　　　　　　　　　　　　　　  |          |        | 　　　　　　　　　　　　　　 |
| ------------ | -------- | -------------------------------------------------------------- | ----------------------------------------------------------- | -------- | ------ | ---------------------------- |
| 日志　　　   | GET/WS　 | `/logs`　　　　　　　　　　　　　　　　　　　　　　　　　　　  | 实时日志，`?level=info\　　　　　　　　　　　　　　　　　　 | warning\ | error\ | debug`，`?format=structured` |
| 流量　　　   | GET/WS　 | `/traffic`　　　　　　　　　　　　　　　　　　　　　　　　　　 | 实时流量 kbps　　　　　　　　　　　　　　　　　　　　　　　 |          |        | 　　　　　　　　　　　　　　 |
| 内存　　　   | GET/WS　 | `/memory`　　　　　　　　　　　　　　　　　　　　　　　　　　  | 实时内存 kb　　　　　　　　　　　　　　　　　　　　　　　　 |          |        | 　　　　　　　　　　　　　　 |
| 版本　　　   | GET　　  | `/version`　　　　　　　　　　　　　　　　　　　　　　　　　　 | 内核版本　　　　　　　　　　　　　　　　　　　　　　　　　  |          |        | 　　　　　　　　　　　　　　 |
| 缓存　　　   | POST　　 | `/cache/fakeip/flush`　　　　　　　　　　　　　　　　　　　　  | 清 fakeip 缓存　　　　　　　　　　　　　　　　　　　　　　  |          |        | 　　　　　　　　　　　　　　 |
| 缓存　　　   | POST　　 | `/cache/dns/flush`　　　　　　　　　　　　　　　　　　　　　　 | 清 dns 缓存　　　　　　　　　　　　　　　　　　　　　　　　 |          |        | 　　　　　　　　　　　　　　 |
| 配置　　　   | GET　　  | `/configs`　　　　　　　　　　　　　　　　　　　　　　　　　　 | 获取基本配置　　　　　　　　　　　　　　　　　　　　　　　  |          |        | 　　　　　　　　　　　　　　 |
| **配置**　   | **PUT**  | **`/configs?force=true`**　　　　　　　　　　　　　　　　　　  | **重载配置（热切换关键）** Body `{"path":"","payload":""}`  |          |        | 　　　　　　　　　　　　　　 |
| 配置　　　   | PATCH　  | `/configs`　　　　　　　　　　　　　　　　　　　　　　　　　　 | 局部更新 Body `{"mixed-port":7890}`　　　　　　　　　　　　 |          |        | 　　　　　　　　　　　　　　 |
| 配置　　　   | POST　　 | `/configs/geo`　　　　　　　　　　　　　　　　　　　　　　　　 | 更新 GEO 库　　　　　　　　　　　　　　　　　　　　　　　　 |          |        | 　　　　　　　　　　　　　　 |
| 重启　　　   | POST　　 | `/restart`　　　　　　　　　　　　　　　　　　　　　　　　　　 | 重启内核　　　　　　　　　　　　　　　　　　　　　　　　　  |          |        | 　　　　　　　　　　　　　　 |
| 升级　　　   | POST　　 | `/upgrade`　　　　　　　　　　　　　　　　　　　　　　　　　　 | 升级内核 `?channel=&force=`　　　　　　　　　　　　　　　　 |          |        | 　　　　　　　　　　　　　　 |
| 升级　　　   | POST　　 | `/upgrade/ui`　　　　　　　　　　　　　　　　　　　　　　　　  | 升级面板（需 external-ui）　　　　　　　　　　　　　　　　  |          |        | 　　　　　　　　　　　　　　 |
| 升级　　　   | POST　　 | `/upgrade/geo`　　　　　　　　　　　　　　　　　　　　　　　　 | 升级 GEO 库　　　　　　　　　　　　　　　　　　　　　　　　 |          |        | 　　　　　　　　　　　　　　 |
| 策略组　　   | GET　　  | `/group`　　　　　　　　　　　　　　　　　　　　　　　　　　　 | 所有策略组　　　　　　　　　　　　　　　　　　　　　　　　  |          |        | 　　　　　　　　　　　　　　 |
| 策略组　　   | GET　　  | `/group/{name}`　　　　　　　　　　　　　　　　　　　　　　　  | 某策略组　　　　　　　　　　　　　　　　　　　　　　　　　  |          |        | 　　　　　　　　　　　　　　 |
| 策略组　　   | GET　　  | `/group/{name}/delay?url=&timeout=5000`　　　　　　　　　　　  | 组内测速，清除自动组 fixed 选择。`?expected=200-299`　　　  |          |        | 　　　　　　　　　　　　　　 |
| 代理　　　   | GET　　  | `/proxies`　　　　　　　　　　　　　　　　　　　　　　　　　　 | 所有代理　　　　　　　　　　　　　　　　　　　　　　　　　  |          |        | 　　　　　　　　　　　　　　 |
| 代理　　　   | GET　　  | `/proxies/{name}`　　　　　　　　　　　　　　　　　　　　　　  | 某代理信息　　　　　　　　　　　　　　　　　　　　　　　　  |          |        | 　　　　　　　　　　　　　　 |
| **代理**　   | **PUT**  | **`/proxies/{name}`**　　　　　　　　　　　　　　　　　　　　  | **选择代理** Body `{"name":"日本"}`　　　　　　　　　　　　 |          |        | 　　　　　　　　　　　　　　 |
| 代理　　　   | DELETE　 | `/proxies/{name}`　　　　　　　　　　　　　　　　　　　　　　  | 清除 fixed 选择（Selector 除外）　　　　　　　　　　　　　  |          |        | 　　　　　　　　　　　　　　 |
| 代理　　　   | GET　　  | `/proxies/{name}/delay?url=&timeout=5000`　　　　　　　　　　  | 单节点测速 `?expected=`　　　　　　　　　　　　　　　　　　 |          |        | 　　　　　　　　　　　　　　 |
| 订阅集合　　 | GET　　  | `/providers/proxies`　　　　　　　　　　　　　　　　　　　　　 | 所有订阅　　　　　　　　　　　　　　　　　　　　　　　　　  |          |        | 　　　　　　　　　　　　　　 |
| 订阅　　　   | GET　　  | `/providers/proxies/{name}`　　　　　　　　　　　　　　　　　  | 某订阅信息　　　　　　　　　　　　　　　　　　　　　　　　  |          |        | 　　　　　　　　　　　　　　 |
| **订阅**　   | **PUT**  | **`/providers/proxies/{name}`**　　　　　　　　　　　　　　　  | **更新/重新拉取该订阅**　　　　　　　　　　　　　　　　　   |          |        | 　　　　　　　　　　　　　　 |
| 订阅　　　   | GET　　  | `/providers/proxies/{name}/healthcheck`　　　　　　　　　　　  | 触发该订阅健康检查　　　　　　　　　　　　　　　　　　　　  |          |        | 　　　　　　　　　　　　　　 |
| 订阅　　　   | GET　　  | `/providers/proxies/{name}/{proxy}`　　　　　　　　　　　　　  | 订阅内某节点信息　　　　　　　　　　　　　　　　　　　　　  |          |        | 　　　　　　　　　　　　　　 |
| 订阅　　　   | GET　　  | `/providers/proxies/{name}/{proxy}/healthcheck?url=&timeout=`  | 订阅内单节点测速　　　　　　　　　　　　　　　　　　　　　  |          |        | 　　　　　　　　　　　　　　 |
| 规则　　　   | GET　　  | `/rules`　　　　　　　　　　　　　　　　　　　　　　　　　　　 | 所有规则　　　　　　　　　　　　　　　　　　　　　　　　　  |          |        | 　　　　　　　　　　　　　　 |
| 规则　　　   | PATCH　  | `/rules/disable`　　　　　　　　　　　　　　　　　　　　　　　 | 临时禁用规则 `{"0":false,"1":true}`　　　　　　　　　　　　 |          |        | 　　　　　　　　　　　　　　 |
| 规则集合　   | GET　　  | `/providers/rules`　　　　　　　　　　　　　　　　　　　　　　 | 所有规则集合　　　　　　　　　　　　　　　　　　　　　　　  |          |        | 　　　　　　　　　　　　　　 |
| 规则集合　   | PUT　　  | `/providers/rules/{name}`　　　　　　　　　　　　　　　　　　  | 更新规则集合　　　　　　　　　　　　　　　　　　　　　　　  |          |        | 　　　　　　　　　　　　　　 |
| 连接　　　   | GET/WS　 | `/connections`　　　　　　　　　　　　　　　　　　　　　　　　 | 连接信息 `?interval=`　　　　　　　　　　　　　　　　　　　 |          |        | 　　　　　　　　　　　　　　 |
| 连接　　　   | DELETE　 | `/connections`　　　　　　　　　　　　　　　　　　　　　　　　 | 关闭所有连接　　　　　　　　　　　　　　　　　　　　　　　  |          |        | 　　　　　　　　　　　　　　 |
| 连接　　　   | DELETE　 | `/connections/{id}`　　　　　　　　　　　　　　　　　　　　　  | 关闭指定连接　　　　　　　　　　　　　　　　　　　　　　　  |          |        | 　　　　　　　　　　　　　　 |
| DNS　　　　  | GET　　  | `/dns/query?name=&type=A`　　　　　　　　　　　　　　　　　　  | DNS 查询　　　　　　　　　　　　　　　　　　　　　　　　　  |          |        | 　　　　　　　　　　　　　　 |
| 存储　　　   | GET　　  | `/storage/{key}`　　　　　　　　　　　　　　　　　　　　　　　 | 读存储，不存在返回 null　　　　　　　　　　　　　　　　　　 |          |        | 　　　　　　　　　　　　　　 |
| 存储　　　   | PUT　　  | `/storage/{key}`　　　　　　　　　　　　　　　　　　　　　　　 | 写存储（合法 JSON，≤1MB）　　　　　　　　　　　　　　　　　 |          |        | 　　　　　　　　　　　　　　 |
| 存储　　　   | DELETE　 | `/storage/{key}`　　　　　　　　　　　　　　　　　　　　　　　 | 删存储　　　　　　　　　　　　　　　　　　　　　　　　　　  |          |        | 　　　　　　　　　　　　　　 |
| DEBUG　　　  | PUT　　  | `/debug/gc`　　　　　　　　　　　　　　　　　　　　　　　　　  | 主动 GC（需 debug 级别）　　　　　　　　　　　　　　　　　  |          |        | 　　　　　　　　　　　　　　 |
| DEBUG　　　  | GET　　  | `/debug/pprof`　　　　　　　　　　　　　　　　　　　　　　　　 | pprof（heap/allocs）　　　　　　　　　　　　　　　　　　　  |          |        | 　　　　　　　　　　　　　　 |

## 热切换订阅关键接口

`PUT /configs?force=true`（重载配置，进程不死、端口不断）：

```bash
curl -X PUT 'http://127.0.0.1:9090/configs?force=true' \
  -H 'Content-Type: application/json' \
  -d '{"path":"config.yaml","payload":""}'
```

- `path` 若不在 mihomo 工作目录，需设 `SAFE_PATHS` 环境变量。
- 配合 `PUT /providers/proxies/{name}` 可强制重新拉取订阅。
