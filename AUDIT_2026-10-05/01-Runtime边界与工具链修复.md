# 一次任务：Runtime 边界与工具链修复

你是负责实际修改代码的工程师。请在当前仓库完成本文件指定阶段，提交可验证的实现和回归测试，不要只输出建议或另写计划。

执行约束：

1. 先阅读本目录 00-审计报告与执行顺序.md，确认基线 SHA，再按本文的文件/函数定位。行号属于审计基线，代码变化后按函数定位。开始前记录 git status；保留用户已有修改，不执行 reset --hard/clean，不删除真实模型/配置/备份。
2. 先写能复现旧行为的回归，确认测试失败原因是本缺陷；再做最小必要修改。不得删除断言、提高资源上限、取消安全 gate 或将失败改为 success 来“通过”测试。
3. 逐项完成下面的修改和验收。复现脚本中的故障桩用于审计证据，不可直接变成生产实现。资源/时钟/IO 桩必须仍通过真实生产入口。
4. 网络、服务、特权测试放在隔离 Linux/OpenWrt 环境；不在真实用户主机停止代理/修改防火墙。允许按现有依赖管理获取工具，不擅自升级全仓库依赖。
5. 保持已有公共协议、快照、配置、权限和回滚契约。必要格式迁移写双向兼容说明与历史 fixture 测试；不能把所有旧状态删掉后“重新训练”。
6. 跑本文测试和触及模块的既有测试，记录命令、环境、返回码。缺环境写“未执行/阻塞”，不能写通过。发现与本文矛盾的证据，给出具体路径和测试结果再调整实现。
7. 结束提供修改文件、各问题修复方式、测试结果和残余风险。未经用户指令不 push/发布/部署。一个任务内按下面的工作顺序逐项完成；每个问题单独写回归、修改和检查点，全部完成再宣告任务结束。

## 合并后的执行方法

合并减少的是需要用户分别启动的任务数，原问题的修改步骤和失败验收全部保留。不要一次无测试地改遍整个仓库。

1. 建立本任务工作清单和修复分支，在下列顺序中一次处理一个问题：先回归、再实现、再验证。可为每个问题做本地提交，但无需让用户为每个问题重新开任务。
2. 复用共同的 IO/锁/时间/事务设施，避免多个问题各自实现不兼容的逻辑；公共 helper 改动后运行全部受影响测试。
3. 若达到上下文限制，在工作清单写清已完成 ID、当前提交、测试命令/结果、剩余事项和恢复入口，继续完成同一任务；不能只因任务合并而漏项。
4. 最终用 ID 对照表逐项交付；任何未完成、环境阻塞和兼容性风险单列。不要把只读验证、未知测量、损坏状态当成功。

工作顺序：RML-01 → RML-04 → RML-03 → RML-02 → RML-05。

五项在一次修改任务内完成。先处理归档与磁盘快照的入口预算，再处理数值状态和反馈时间，最后修 SBOM 的 UTF-8 IO。保持 Frozen core 的稳定 API/历史状态契约；CLI、Archive 和 Preview Runtime 的变更必须经过两套 feature 测试。

## RML-01 · P1 · ZIP 实际解压字节不受声明大小限制

定位：`crates/rill-runtime/src/archive.rs:317–370，ModelPack/HandlerPack 的归档读取共享入口`。

审计证据：已复现：构造有效 CRC 的 DEFLATE manifest，实际约 1 MiB，local/central directory 声明未压缩大小为 1。预检按声明通过，read_to_end 将全部内容读入；rill-pack verify 的 JSON 错误位置达 column 1048578，而不是资源上限拒绝。发生在签名验证前。不需要攻击者持有签名私钥。

### 必须实现

1. 元数据预检继续保留，但每个解压流必须使用有界读取，最多读 min(单文件上限、剩余总量、实际压缩比预算)+1；超出立即报资源限制，不能先 read_to_end 再看长度。
2. 实际字节计数做 checked arithmetic；读取结束比较实际长度与声明长度，并保持 CRC/ZIP 错误传播。总量用实际读取量累计。处理 compressed_size=0、超大 u64、加法溢出、重复 member、无效压缩方法。
3. manifest/signature/metadata 也受相同限制，签名前 JSON 和 WASM 不得分配超限内存。限制错误类型可稳定辨识，不泄露模型内容。
4. 同时修 ModelPack 和 HandlerPack 路径；保留路径、重复项、签名和模型摘要校验，不通过把现有 max 放大解决。
5. 不改变 Frozen RillML core 序列化/API，不替换密码算法。本问题在 Runtime 归档 IO 层。

### 验收测试

- 真实生成 under-declared ZIP：实际 1 MiB/声明 1、正确 CRC，必须在 JSON/签名处理前以 limit/size-mismatch 拒绝。
- 实际恰好上限可接受，超过 1 字节拒绝；总量由多 member 累计超限也拒绝。
- 声明过大、过小、零压缩大小、CRC 错、重复路径、压缩比超限、整数溢出全部拒绝。
- 合法已签名 model/handler fixture 完整验签与执行成功。测试不要真实分配 GB 级内容，用中等 fixture/reader fault 实证有界读取。
- cargo test -p rill-runtime --locked（默认 wasm）和无默认 feature 都运行。

## RML-04 · P2 · Preview CLI 在资源校验前无界读取快照文件

定位：`crates/rill-runtime/src/bin/rill-runtime.rs:626–629；stateful.rs resource profile/max_snapshot_bytes`。

审计证据：静态确认：fs::read 整个 --state-file 后 serde_json 解析，再交给 engine 的快照资源限制。超大文件在限额检查前已分配内存。ResourceProfile 的 512 KiB 等内部预算不能约束这次磁盘 IO/JSON 分配；嵌套 previous_good/candidate 字节数组还有 JSON 展开体积。

### 必须实现

1. 对原始快照 JSON 文件使用有界流读取，上限为明确的磁盘序列化预算 +1，并在 serde 前拒绝超限。metadata 可预检但不能代替流计数，文件可能在读取时增长。
2. 区分模型二进制、内部 snapshot 与磁盘 JSON 展开后的预算，文档列出各预算及合法最大状态。不得直接把内部 512 KiB 当原始 JSON 大小而拒绝所有历史最大合法快照。
3. 反序列化后再验证嵌套状态、历史快照、pending/completed 条目数量和实际字节总量，确保所有资源约束仍在。
4. 拒绝时不覆盖原状态文件、不新建空状态偷换为训练成功。明确错误并保留人工诊断路径。
5. 保存也使用同一磁盘预算/原子写入，避免自身产生下次启动无法加载的快照。

### 验收测试

- 文件大小在预算-1/预算/预算+1；文件读取过程中增长；稀疏巨文件：超限在 parse 前失败且读取字节不超过预算+1。
- 嵌套 previous_good/candidate/ledger 巨数组拒绝，合法历史快照和最大合法状态可恢复。
- 损坏快照不被静默覆盖；重启后有效 generation/ledger 幂等性不变。
- CLI 测试需真实走 --state-file，不能只测试 engine.deserialize。

## RML-03 · P1 · 有限特征的乘加溢出产生 accepted 的 null score

定位：`crates/rill-runtime/src/bin/rill-runtime.rs:376–380、452–457 及 builtin learner feedback/snapshot restore`。

审计证据：已复现：两个 1e308 特征本身 finite；feedback 后 weights 被 clamp 为 1000，再决定时 dot product 溢出，输出 score:null，同时 accepted=true 并增加 generation。Infinity 经 serde_json 转成 null，候选比较/排序/下游学习失去数值契约。

### 必须实现

1. 对 dot product 的每次乘法与累计结果、feedback prediction/error/weight delta 检查 finite；发现溢出返回 typed error，而非继续选择候选或只 clamp 最终数值。
2. 计算候选响应和下一状态使用临时副本，全计算成功后再提交。错误不能留下部分 action/features/weight/counter 更新。
3. 根据 builtin 既有契约决定是否添加特征幅度上限；有限并不代表可安全计算。不能假定所有宿主都已归一化，不能盲目把异常 clamp 成优质分数。
4. 恢复 builtin snapshot 时验证 featureCount、weights/bias、action features/counters 等结构和数值，拒绝不一致状态；保持 handlerStateVersion=2 的合法历史数据可恢复。
5. 输出 accepted=true 必須意味着所有 score 是有限 JSON number。明确“不存在分数”与计算错误，不能以 null 代表 Infinity。

### 验收测试

- 正负 1e308、多项累计溢出、乘法溢出、feedback 更新溢出均拒绝并保持原状态与 generation。
- 正常小特征/合法边界分数仍正确，排序稳定且 tie-break 不变。
- snapshot 含极端有限 weight、错误长度、错误计数、非法数据形态时被拒绝，合法旧快照可读。
- 运行真实 preview serve 复现序列，断言响应 score 不再是 null；core 的 Frozen tests 全部通过。

## RML-02 · P1 · Stateful Feedback 未校验结果时间

定位：`crates/rill-runtime/src/stateful.rs:1143–1195；crates/rill-runtime-protocol/src 的 FeedbackV3/请求验证`。

审计证据：已复现：真实 preview Runtime 决策后，outcomeTimeMs=0 的 feedback 返回 accepted=true 并训练；远未来 999999999999999 同样可接受。已有身份、generation、选中 action 校验不替代时间有效性。created_at 已保存在决策 ledger。

### 必须实现

1. 在调用 handler/修改模型/标记 completed 前，将 outcomeTimeMs 与该 decision 的 created_at 和当前可信时钟比较：不能早于 decision，不能超过当前时间加明确的时钟偏差容限。
2. 将时钟注入或集中读取，使边界测试可确定。使用同单位毫秒与 checked arithmetic，不混 epoch seconds、monotonic、deadline。
3. 如文档确实约定反馈最大寿命，落地可配置的 Preview 策略并保存决策有效期；没有既有 TTL 契约时不要擅自给 Stable 协议增加强制短 TTL。至少修复确定不可能的先发生/远未来时间。
4. 无效时间返回明确 error，state/model generation、pending/completed ledger、snapshot 和模型字节不变。重试正确时间仍可完成该 pending decision。
5. 重复 feedback 继续保持原来的 exactly-once 语义；不把第一次无效反馈写成已处理。保留旧 snapshot 可读。

### 验收测试

- created_at-1、0、当前时间+容限+1 拒绝；created_at、容限边界及正常延迟反馈接受。
- 用固定测试时钟，无 sleep；毫秒最大值与溢出拒绝。
- 错误反馈后 snapshot 哈希、generation、model 完全不变，后续合法反馈训练恰好一次。
- 若实现 TTL，过期边界、重启、旧 snapshot 迁移都有明确测试。所有 v3 ledger/delayed-feedback 测试通过。

## RML-05 · P2 · SBOM 生成依赖 Windows 默认文本编码导致失败

定位：`scripts/generate_sbom.py:21–28（cargo metadata subprocess）`。

审计证据：已复现：Windows 中文默认 GBK 环境中 Python 脚本测试失败，cargo metadata UTF-8 描述包含字节 0x94，subprocess(text=True) 的读线程 UnicodeDecodeError，随后 stdout=None 导致 json.loads TypeError。126 个 Python 测试中该项是代码问题；resource 模块不存在的另一项属于 Linux 测试环境限制。

### 必须实现

1. 所有来自 Cargo/Rust 工具的结构化输出显式指定 encoding="utf-8" 与明确错误策略。不要依赖 PYTHONUTF8=1 或系统 locale 来“修复”代码。
2. Cargo 非零、UTF-8 非法、JSON 非法分别给明确诊断；不能在读线程失败后产生无关 NoneType 栈迹。
3. JSON/TOML/清单读写采用显式 UTF-8，检查同目录其他 cargo metadata wrapper，避免相同遗漏。保留 SBOM 内容、依赖图、许可证和确定性排序。
4. 不更改无关 Linux resource 测试以假装全平台支持；将平台专用测试与可移植生成器的契约分别记录。

### 验收测试

- 模拟 Cargo UTF-8 输出包含中文、弯引号、非 ASCII description，宿主默认 GBK 时生成成功。
- PYTHONUTF8=0 的 Windows 真实 cargo metadata 路径成功；Linux UTF-8 环境输出一致。
- 非零退出/非法 UTF-8/损坏 JSON 清晰失败，不生成部分 SBOM。
- scripts/tests 的 SBOM 与 release-index 测试通过，重复生成确定性内容一致。

## 任务完成条件

为本文每个问题提交实现、回归和结果；输出“ID → 修改位置 → 正常/失败验证 → 未完成项”的清单。生产实现可独立复核、历史数据兼容、错误不假成功。新增本任务修复记录，保留审计证据原文。需要后续阶段时写出 exact commit 和下一份提示词文件名。

## 建议的实际验证入口

仓库根目录运行，使用锁文件；需要 Python 的检查先配置本机对应 PYO3_PYTHON。新增边界测试先跑，再跑下列既有测试。

```sh
cargo test -p rill-runtime --locked
cargo test -p rill-runtime -p rill-runtime-protocol -p rill-ml-ffi --no-default-features --locked
cargo test -p rill-ml --features serde --locked
cargo check --workspace --locked
python3 -m unittest discover -s scripts/tests
```

默认 wasm 与 no-default-features 是两套覆盖，都保留；Frozen core、历史 state fixtures、release index/签名和真实 Runtime qualification 按仓库现有脚本/CI 再执行。Linux resource 测试在 Linux 跑，Windows UTF-8 回归在 PYTHONUTF8=0 的 Windows 跑，不能以一种平台替代全部资格。
