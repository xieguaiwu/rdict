---
name: iterative-improvement-loop
version: 1.0.0
description: 对代码库执行"修改→审查→修复→审查"的迭代改进循环，直至审查通过
triggers:
  - "改进循环"
  - "迭代优化"
  - "持续重构"
  - "improvement loop"
  - "refactoring loop"
  - "修复循环"
  - "迭代改进"
inputs:
  - name: target_path
    description: 要改进的目标代码库路径（目录或文件）
    required: true
  - name: max_iterations
    description: 最大循环次数，防止无限循环
    required: false
    default: 5
  - name: review_focus
    description: 审查重点，如 "architecture, code quality, and extensibility"
    required: false
    default: "code architecture, code quality, potential issues, and extensibility"
  - name: improvement_goal
    description: 改进目标描述，为空则由审查自主发现
    required: false
    default: ""
tools:
  - read_file
  - list_dir
  - grep_files
  - file_search
  - write_file
  - edit_file
  - apply_patch
  - exec_shell
  - task_shell_start
  - task_shell_wait
  - agent_spawn
  - agent_result
  - agent_wait
  - agent_cancel
  - agent_send_input
  - checklist_write
  - checklist_update
  - update_plan
  - diagnostics
---

# 迭代改进循环 (Iterative Improvement Loop)

## 任务目标

对目标代码库执行一个自动化的闭循环：**修改 → 审查 → 修复 → 再审查**，直到审查判断无问题后退出。适用于以下场景：

- 对一段代码做完一轮修改后，希望检查是否存在疏漏或架构问题
- 希望持续打磨代码质量，直到满足架构标准
- 对大型重构进行分轮验证，逐步逼近理想状态
- 在实现新功能后，系统性检查可扩展性和边界情况

## 核心设计

```
┌──────────────────────────────────────────┐
│  1. 执行一轮修改（由用户主导或自主完成）      │
└─────────────┬────────────────────────────┘
              ▼
┌──────────────────────────────────────────┐
│  2. 启动审查 subagent                     │
│     检查：架构、质量、问题、可扩展性          │
│     发现 → 输出问题列表                    │
│     无问题 → 标记 review_passed=true       │
└─────────────┬────────────────────────────┘
              ▼               ┌───────────┐
        有问题? ──────是────▶ 3. 启动修改  │
              │               subagent    │
              │               └─────┬─────┘
              │                     ▼
             否              回到步骤 2
              │
              ▼
┌──────────────────────────────────────────┐
│  4. 循环结束                              │
│     输出：审查报告 + 修改摘要              │
└──────────────────────────────────────────┘
```

## 执行流程

### 阶段 0：初始化

1. 读取 `target_path` 了解项目结构
2. 记录当前 git commit hash（如有 git 仓库），用于后续对比
3. 获取 `max_iterations` 和 `review_focus` 参数
4. 设置迭代计数器 `iteration = 0`
5. 通过 `checklist_write` 创建可视化进度追踪

### 阶段 1：执行修改（由用户触发或自主完成）

在本阶段完成一轮代码修改。修改范围可以是：

- 用户直接提出的修改请求
- 前一轮审查发现的问题修复
- 基于 `improvement_goal` 的自主改进

每次修改后，使用 git 记录变更：
```bash
git add -A && git commit -m "iter-improve-loop: iteration {n} modifications"
```

### 阶段 2：审查（Review Subagent）

#### 2.1 启动审查 subagent

使用 `agent_spawn` 创建一个审查 subagent，`fork_context: true` 以继承上下文。

**Subagent 任务描述**：

```
你是一个代码审查 subagent。请对 {target_path} 下的代码进行审查。

审查重点（{review_focus}）：

1. 代码架构层面：
   - 模块划分是否合理？
   - 有无循环依赖或职责不清？
   - 接口是否稳定、易于扩展？

2. 代码质量层面：
   - 有无潜在 bug 或逻辑错误？
   - 有无重复代码、死代码、过度工程？
   - 命名是否清晰、注释是否有价值？

3. 边界情况：
   - 有无遗漏的输入验证？
   - 错误处理是否完善？
   - 有无空指针、越界、并发问题？

4. 可扩展性：
   - 是否方便添加新功能？
   - 依赖关系是否可控？
   - 是否过度耦合？

输出格式：
- 如果无问题：输出 "REVIEW_PASSED: true"
- 如果有问题：输出问题列表，每个问题包含：
  - [严重程度: critical/major/minor]
  - [文件路径:行号]
  - [问题描述]
  - [建议修改方向]

请使用 read_file、list_dir、grep_files 等工具阅读代码。
```

#### 2.2 获取审查结果

通过 `agent_wait` 等待 subagent 完成，然后调用 `agent_result` 获取完整结果。

#### 2.3 判断是否通过

- 如果 subagent 输出包含 `REVIEW_PASSED: true` → **循环结束**
- 如果 subagent 输出问题列表 → 进入阶段 3

#### 2.4 处理特殊情形

| 情形 | 处理 |
|------|------|
| subagent 超时或失败 | 重新启动一次，仍失败则标记结果不可靠并询问用户 |
| 问题过多（>20 条） | 仅选取 critical + major 级别的处理，剩下的留到下一轮 |
| `iteration >= max_iterations` | 强制退出循环，输出已完成内容和剩余问题 |

### 阶段 3：修复（Fix Subagent）

#### 3.1 拆分问题

将审查发现的问题按严重程度排序：

1. **critical**：直接 bug、空指针、安全漏洞、数据丢失风险
2. **major**：架构缺陷、接口设计问题、复杂度过高、异常处理缺失
3. **minor**：命名不当、注释缺失、代码风格、轻微冗余

**分批策略**：每轮修复一批问题，批次上限按问题数量灵活决定：
- 1-5 个问题 → 全部修复
- 6-15 个问题 → 先修 critical + major
- 16+ 个问题 → 每轮修最多 10 个问题

#### 3.2 启动修复 subagent

对每个批次，启动一个修改 subagent：

```
你是一个代码修改 subagent。请根据以下问题列表修复 {target_path} 的代码。

问题列表：
[{问题1}]
[{问题2}]
...

要求：
1. 每个修改请使用 read_file 先确认代码上下文，再用 edit_file 或 apply_patch 修改
2. 修改前用 git 快照：git add -A && git commit -m "..."
3. 修改后用 git commit 记录变更
4. 不要修改与问题无关的代码
5. 如果一个问题需要多个文件联动修改，请一次性完成
```

#### 3.3 验证修复

修复完成后，快速验证：
- 修改的文件是否存在语法错误（通过 `exec_shell` 运行编译器/检查工具）
- 问题文件中描述的变更是否确实被执行（抽查 read_file）

如有语法错误，在同一 subagent 内修复。

### 阶段 4：循环

`iteration += 1`，回到阶段 2。

### 阶段 5：完成

循环结束后，输出：

```
=== 迭代改进循环完成 ===
循环次数: {n}
目标路径: {target_path}

=== 修改摘要 ===
{列出主要变更}

=== 审查结论 ===
{审查 subagent 的最终评价}

=== 剩余问题（如有） ===
{被标记为 "留待后续" 的问题列表}
```

## 输出格式

### 每轮迭代摘要（循环进行中）

```
━━━ 迭代 {n}/{max_iterations} ━━━
发现 {m} 个问题：
  [critical] file.rs:42 — ...
  [major]    mod.rs:15 — ...
  [minor]    file.rs:88 — ...
已修复 {k} 个问题，进入下一轮审查
```

### 循环结束报告

```
=== 迭代改进循环完成 ===
循环次数: 3
目标路径: /path/to/project

=== 修改摘要 ===
- 重构了模块 A 的接口，消除了循环依赖
- 为所有 API 端点添加了输入校验
- 提取了重复的逻辑为公共工具函数
- 补充了 12 处错误处理和 3 处并发保护

=== 审查结论 ===
REVIEW_PASSED: true — 架构合理、无关键问题、代码质量达标

=== 变更文件 ===
- src/module_a.rs (6 处修改)
- src/module_b.rs (3 处修改)
- src/utils.rs (新建)
```

## 注意事项

### 防止无限循环

1. **`max_iterations` 硬上限**：达到后强制退出，无论审查是否通过
2. **问题数量递减检查**：如果连续两轮发现的问题数量不减反增，说明修改方向不对，应退出循环并报告
3. **相同问题重复出现**：如果同一问题在连续两轮审查中都出现且未被有效修复，标记为 "顽固问题"，跳出循环让用户决策
4. **空循环保护**：如果没有做任何修改却通过了审查的第一轮，这算通过（不做无谓循环）

### Subagent 管理

1. **超时处理**：审查 subagent 设置合理超时（建议 120 秒），超时则重新启动一次
2. **上下文隔离**：每个 subagent 使用 `fork_context: true` 继承主上下文
3. **结果验证**：修改 subagent 返回后，抽查 1-2 个修改点确认变更真正落地
4. **不要并行**：审查和修复是串行的——下一轮必须等上一轮审查结果出来后再决定

### Git 安全网

本 skill 遵守 [Git 安全网规范](git_safety_net.md)。每次修改前执行 git 快照，每次修改后提交。确保所有变更都可追溯、可回滚。

### 适用边界

| 场景 | 适用性 |
|------|--------|
| 代码重构 | ✅ 核心场景 |
| 功能实现后质量检查 | ✅ 高度适用 |
| 修复 bug 后验证 | ✅ 适用 |
| 新增 API 端点 | ✅ 可用 |
| 纯文档修改 | ⚠️ 只检查结构和链接 |
| 二进制/配置文件 | ❌ 不适用 |
| 首次代码编写 | ❌ 不是从零生成，而是改进已有代码 |

### 与用户互动

1. **首次启动时**：输出当前代码状态的摘要（文件数、行数、复杂度粗略评估）
2. **发现问题时**：列出问题并简要说明每个问题的风险，让用户了解循环在做什么
3. **强制退出时**：解释为什么退出（达到迭代上限 / 问题不减反增 / 顽固问题），给用户后续建议
4. **用户可随时中断**：一旦用户介入给出新指令，停止当前循环按新指令执行
