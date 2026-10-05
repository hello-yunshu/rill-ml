# Runtime 边界与工具链修复记录

基线：`audit/2026-10-05` HEAD `c532f4e`。审计报告中的代码基线为 `9602750202f56ed860fc8815bb91e81083d0ffdb`；两者之间只新增审计文档与 evidence。
修复分支：`audit/2026-10-05-runtime-toolchain-fixes`

| 顺序 | ID | 修改位置 | 回归、修复和验证 |
|---|---|---|---|
| 1 | RML-01 | `crates/rill-runtime/src/archive.rs` | 旧行为复现先失败：带正确 CRC 的 1 MiB DEFLATE stream 将 local/central 未压缩长度都伪报为 1，旧读取入口完整读入并返回 Ok。共享入口现在按 file/total/ratio 最小预算 +1 流读、对实际字节 checked 累计并与声明大小比对；CRC、ZIP 方法和大小错误仍传出。测试覆盖单文件边界、one-over、累计总量、under/over-declared、ratio、0 compressed、乘法溢出、CRC、无效 method、重复项（handler 测试）和签名包往返。默认 runtime 全套通过；无默认 feature runtime/protocol/ffi 全套通过。 |
| 2 | RML-04 | `crates/rill-runtime/src/bin/rill-runtime.rs`、`docs/preview-v3-state-snapshot-budgets.md` | Preview CLI 将原始 JSON 限为 2 MiB，load 最多读取预算 +1 后才 parse；save 检查相同预算后再开临时文件。内部单 state 256 KiB、总 state 8 MiB、每分区 ledger 上限和 512 KiB compact snapshot 预算保留并在文档列明。回归覆盖预算−1/预算有效快照恢复、预算+1 稀疏文件在 parse 前拒绝、读入期间增长最多预算+1、超限嵌套 previous_good 拒绝、保存超限保留原文件；真实 CLI 冷重启恢复通过。 |
| 3 | RML-03 | `crates/rill-runtime/src/bin/rill-runtime.rs` | builtin handler 在使用前校验 version 2 state、计数器、featureCount、weights/bias 范围和 action feature 结构；dot product 乘法/累加与 feedback prediction/error/delta/weight/bias 更新逐步检查 finite。计算错误不会把临时 state 提交给 Runtime。测试覆盖正负极端 feature、乘法/累加/prediction/update overflow、畸形快照和合法 version 2 快照。真实 Preview CLI 溢出序列返回 error，无 score:null、generation 不变且 state file 字节不变。 |
| 4 | RML-02 | `crates/rill-runtime/src/stateful.rs` | 在调 handler 之前，用传入的 Unix 毫秒时钟拒绝 `outcomeTimeMs < createdAt` 或大于 `now + 5 min`；checked-add 时钟溢出也 fail closed。没有增加 feedback TTL。固定时钟回归覆盖 createdAt−1、0、u64::MAX、now+容限+1、createdAt 和容限边界；失败前后完整 runtime snapshot 相同，正确 retry exactly-once。真实 CLI 错误 feedback 后持久文件不变，正确 retry 完成一条 ledger。 |
| 5 | RML-05 | `scripts/generate_sbom.py` 及 Cargo 输出 wrappers、`scripts/tests/test_generate_sbom.py` | Cargo 元数据改为 bytes 并严格 UTF-8 解码；非零退出、非法 UTF-8、JSON 错误各有清楚诊断，失败前不创建 SBOM 目录。Cargo/Rust 工具 wrapper 显式 UTF-8。新增 Unicode fixture 和错误分支测试。Windows Python 3.12.14，`PYTHONUTF8=0` / 默认 `cp936` 下以真实 Cargo metadata 连续生成两次，文件 hash 相同；`verify_sbom.py` PASS。定向 SBOM 测试 4 项通过。 |

## 最终验证

- `cargo test -p rill-runtime --locked`：通过（85 library、9 binary、13 process、5 Stateful Handler、17 WASM、6 WIT fixture tests）。
- `cargo test -p rill-runtime -p rill-runtime-protocol -p rill-ml-ffi --no-default-features --locked --quiet`：通过。
- `cargo test -p rill-ml --features serde --locked`：通过（759 unit、全部 integration suites、42 doctests）。
- `cargo check --workspace --locked`（`PYO3_PYTHON` 指向随 Codex 提供的 Python）：通过。
- Windows `python -m unittest discover -s scripts/tests`：129 项中 128 项通过；唯一未通过项是 Windows 没有 Linux 专用 `resource` 模块。该模块之后已在 WSL 定向运行。
- `rustfmt --check --config skip_children=true` 对本次触及的 Rust 文件通过。`cargo fmt --all -- --check` 报告多个未修改仓库文件存在预存换行风格错误；没有格式化全仓。

修复前基线复现和平台限制另见审计原件 `00-审计报告与执行顺序.md` 及 `evidence/`。未执行真实 OpenWrt/代理/DAC/PID1 安装矩阵；未 push、部署、发布或合并。

### WSL 补充验证

按后续授权安装了 Ubuntu 24.04 WSL、Python 3.12.3、Rust/Cargo 1.94.0 和本机构建依赖；仓库在 WSL Linux 文件系统中的独立检出与 Windows 修复分支同为提交 `e4f299d`。

- `cargo test -p rill-runtime --locked`：Linux 通过（85 library、9 binary、13 process、5 Stateful Handler、17 WASM、6 WIT fixture tests）。
- `python3 -m unittest discover -s scripts/tests -p test_runtime_qualification.py -v`：11 项通过，包括 Linux `resource` 导入、资格结果断言、required partition key 和 Unix 毫秒时钟回归。
- `python3 scripts/run_runtime_qualification.py --runtime target/debug/rill-runtime --observations 2 --json`：PASS；真实 Linux 子进程完成 Preview handshake、2 条决策、重启恢复、反馈和重复反馈拒绝。
- 上述实际 smoke 首次暴露两个旧脚本夹具问题：envelope 漏 `partitionKey`；feedback 继续使用小整数时间戳，违反 RML-02。现已在两个资格脚本统一加入 `partitionKey=default` 和当前 Unix 毫秒时间，并加入回归断言；修复后的 smoke 与 11 项定向 Python 测试通过。
- 完整 `scripts/tests` 套件在首个 SBOM 集成用例执行真实 `cargo metadata` 时启动了工作区索引扫描，超过 10 分钟仍未完成，已停止；因此 Linux 全套仍标记为未执行。`run_runtime_final_qualification.py` 含 1,025 条容量探测和最长 4,096 次同状态饱和循环，文档列为 push 后压力资格；本任务没有 push，故未执行该重负载阶段。

没有启用 Docker Desktop Linux Engine，也未执行真实 OpenWrt/代理/DAC/PID1 安装矩阵。未 push、部署、发布或合并。
