# Preview v3 状态快照资源预算

Preview CLI 在读取 `--state` 文件时，先以流式方式读取最多 `2 MiB + 1` 字节。超过 2 MiB 会在 JSON 反序列化前失败。2 MiB 是磁盘 JSON 预算，包含 UTF-8 JSON 本身、数组展开和历史文件空白；它不替代 Runtime 内部资源限制。

默认 `ResourceProfileV1` 的运行时约束如下：

| 资源 | 默认上限 | 说明 |
|---|---:|---|
| 分区 | 64 | 一个快照内的 client/partition namespace 数 |
| 单份 handler state | 256 KiB | 当前、previous-good、candidate 均分别受此限 |
| handler state 总字节 | 8 MiB | 跨分区及历史状态累计值 |
| pending decisions | 每分区 1,024 | 以 ledger entry 数计 |
| completed decisions | 每分区 4,096 | 以 ledger entry 数计 |
| Runtime 快照 JSON | 512 KiB | compact `serde_json` 表示；包含分区、ledger、历史状态和校验和 |
| Preview 磁盘 JSON | 2 MiB | 在 parse 前限制原始文件字节；读写共用此上限 |

`Vec<u8>` handler state 在磁盘格式中以 JSON 数字数组编码，因此它的 UTF-8 文件大小可能高于模型二进制大小。Runtime 仍按单状态、总状态、ledger 数量和 512 KiB compact 快照预算验证反序列化结果；超过任一内部预算的旧文件也会拒绝恢复。CLI 保存时先在内存生成 compact JSON 并检查 2 MiB 预算，再写临时文件并原子替换旧快照；检查或写入失败不会把旧快照改成空状态。

2 MiB 磁盘限额为当前 512 KiB 最大 compact 快照留有 JSON 展开和空白空间。以前生成的 compact Preview 快照可直接读取，无格式迁移；handler `handlerStateVersion=2` 继续使用原格式。
