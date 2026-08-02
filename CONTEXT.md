# easyT Translation

This context describes how easyT presents model-backed translations to the user.

## Language

**翻译后端**:
easyT 取得模型译文所采用的接入方式。Official API 与 Qwen 网页实验模式是翻译后端；具体供应商和模型不属于翻译后端。
_Avoid_: 模型、模型类型

**流式输出**:
一种译文展示策略：当前翻译请求的可见译文随着可用内容持续更新，直到翻译完成或失败。它不约束翻译后端采用何种传输协议。
_Avoid_: 流式传输、SSE 模式

**一次性输出**:
一种译文展示策略：仅在翻译完整成功后向用户展示译文，处理中不展示部分译文。
_Avoid_: 非流式传输

**未完成译文**:
流式输出过程中已经可见、但请求未能完整成功的译文。它可以保留供用户查看，但必须明确标记且不可作为完整译文复制。
_Avoid_: 失败结果、最终译文
