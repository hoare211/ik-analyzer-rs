use crate::core::analyze_context::AnalyzeContext;
use crate::core::lexeme::{Lexeme, LexemeRef};
use crate::core::lexeme_path::LexemePath;
#[cfg(any(test, feature = "judge-fast"))]
use std::cmp::Ordering;

/// 候选路径的聚合指标（O(1) 合成，详见 v3 §二·性质 3）。
/// 与 `LexemePath::cmp` 六条规则一一对应；`path_begin` 在 `t_i=0` 时取 `lexemes[ci].begin()`，
/// 故非常量，规则 3 必须真实计算 `path_end - path_begin`。
#[derive(Clone, Copy)]
#[cfg(any(test, feature = "judge-fast"))]
struct PathMetrics {
    payload: i64,
    size: i64,
    path_begin: i64,
    path_end: i64,
    x: i64,
    p: i64,
}

/// 复刻 `LexemePath::cmp` 的六条规则（规则 5/6 的 `x/p` 越大越好）：
/// 返回 `Less` 表示 `a` 比 `b` 更优（与 `&option < &best` 的语义一致）。
#[cfg(any(test, feature = "judge-fast"))]
fn cmp_metrics(a: &PathMetrics, b: &PathMetrics) -> Ordering {
    // 规则 1：payload_length 越大越好
    b.payload
        .cmp(&a.payload)
        // 规则 2：size 越小越好
        .then_with(|| a.size.cmp(&b.size))
        // 规则 3：path_length（= path_end - path_begin）越大越好
        .then_with(|| (b.path_end - b.path_begin).cmp(&(a.path_end - a.path_begin)))
        // 规则 4：path_end 越大越好
        .then_with(|| b.path_end.cmp(&a.path_end))
        // 规则 5：x_weight 越大越好
        .then_with(|| b.x.cmp(&a.x))
        // 规则 6：p_weight 越大越好
        .then_with(|| b.p.cmp(&a.p))
}

/// 饱和乘积：上限 `i32::MAX`，与参考实现 `try_fold(checked_mul).unwrap_or(i32::MAX)` 等价
/// （长度恒 ≥1，乘积单调不减，"一旦溢出即永远 MAX"）。
#[cfg(any(test, feature = "judge-fast"))]
fn sat_mul(a: i64, b: i64) -> i64 {
    a.checked_mul(b).unwrap_or(i64::MAX).min(i32::MAX as i64)
}

/// 把 `v` 重置为「长度 `n`、每元素均为 `value`」——scratch 复用的唯一入口。
///
/// **必须 `clear()` 再 `resize()`**：`Vec::resize` 在 `new_len <= len` 时只截断、
/// **不会**用 `value` 覆写已有元素，直接 `resize` 会留下上一次调用的陈旧值。
#[cfg(any(test, feature = "judge-fast"))]
#[inline]
fn reset_scratch<T: Clone>(v: &mut Vec<T>, n: usize, value: T) {
    v.clear();
    v.resize(n, value);
}

/// `judge_fast` 的常驻暂存区（评审 P2-2）。
///
/// 原实现在函数体内新建 15 个 `Vec`，**每次调用约 15 次堆分配**；而
/// `judge_reference` 复用 `conflict_stack`/`fp_scratch`（每次仅 2 次）。
/// 既然 `judge_fast` 是默认生产路径，就应与参考实现同一水平。
///
/// 复用安全性：所有数组在 `judge_fast` 中都只做**下标覆盖写**（不依赖旧值），
/// 故「clear + resize + 覆写」与原先的 `vec![init; n]` 逐位等价。
/// 唯一需要显式清零的是 `in_chain0`（用 `false` 填充）。
#[cfg(any(test, feature = "judge-fast"))]
#[derive(Default)]
struct FastScratch {
    begins: Vec<i32>,
    next: Vec<usize>,
    payl: Vec<i64>,
    sz: Vec<i32>,
    xw: Vec<i64>,
    pw: Vec<i64>,
    pend: Vec<i32>,
    chain0: Vec<usize>,
    in_chain0: Vec<bool>,
    pl: Vec<i64>,
    ps: Vec<i64>,
    pxw: Vec<i64>,
    ppw: Vec<i64>,
    chain0_end: Vec<i32>,
    chosen: Vec<usize>,
}

/// `judge_reference` 的常驻暂存区（原先每次调用 2 次堆分配）。
#[cfg(any(test, feature = "judge-verify", not(feature = "judge-fast")))]
#[derive(Default)]
struct RefScratch {
    conflict_stack: Vec<usize>,
    fp_scratch: Vec<usize>,
}

/// IK分词歧义裁决器
///
/// 携带一组常驻 scratch，使 `judge` 的临时数组在多次调用间复用（评审 P2-2）。
/// 每个 `IKSegmenter` 独占一个实例，故复用无需同步；`IKArbitrator` 不含
/// 跨线程共享的可变状态。
pub(crate) struct IKArbitrator {
    #[cfg(any(test, feature = "judge-fast"))]
    fast: FastScratch,
    #[cfg(any(test, feature = "judge-verify", not(feature = "judge-fast")))]
    reference: RefScratch,
}

impl IKArbitrator {
    pub fn new() -> Self {
        Self {
            #[cfg(any(test, feature = "judge-fast"))]
            fast: FastScratch::default(),
            #[cfg(any(test, feature = "judge-verify", not(feature = "judge-fast")))]
            reference: RefScratch::default(),
        }
    }

    pub fn process(&mut self, context: &mut AnalyzeContext, use_smart: bool) {
        let mut cross_path = LexemePath::new();
        // 方案A：一次性取走全部词元（O(1)），避免逐个 poll_first/remove(0) 的 O(n²) 元素搬移。
        for org_lexeme in context.org_lexemes_mut().drain_all() {
            // org_lexeme 为 LexemeRef（Copy）。add_cross_lexeme 接收 Lexeme，需包装。
            if !cross_path.add_cross_lexeme(Lexeme::from_ref(org_lexeme)) {
                if cross_path.size() == 1 || !use_smart {
                    context.add_lexeme_path(cross_path);
                } else {
                    let judge_result = self.judge(&cross_path);
                    context.add_lexeme_path(judge_result);
                }
                cross_path = LexemePath::new();
                cross_path.add_cross_lexeme(Lexeme::from_ref(org_lexeme));
            }
        }
        if !cross_path.is_empty() {
            if cross_path.size() == 1 || !use_smart {
                context.add_lexeme_path(cross_path);
            } else {
                let judge_result = self.judge(&cross_path);
                context.add_lexeme_path(judge_result);
            }
        }
    }

    // ---- 正式入口：judge_fast 就绪后由 `judge-fast` feature 接管；否则回落 reference ----

    /// 双跑校验模式（`judge-fast` + `judge-verify` 同时开启）：对**每一个真实 cross_path**
    /// 同时执行两实现并逐位比对，分歧立即 panic 并打印输入以便复现。
    /// 用途：在任意真实语料上做全覆盖差分（形态完全由语料决定，不依赖随机构造）。
    /// 代价：耗时约为单跑之和，**仅验证用**，不可用于生产或计时。
    #[cfg(all(feature = "judge-fast", feature = "judge-verify"))]
    fn judge(&mut self, cross_path: &LexemePath) -> LexemePath {
        fn sig(p: &LexemePath) -> Vec<(i32, i32)> {
            p.lexemes()
                .as_slice()
                .iter()
                .map(|l| (l.begin(), l.length()))
                .collect()
        }
        let fast = self.judge_fast(cross_path);
        let reference = self.judge_reference(cross_path);
        assert_eq!(
            sig(&fast),
            sig(&reference),
            "[judge-verify] judge_fast 与 judge_reference 在真实语料上分歧：\ninput={:?}\nfast={:?}\nref={:?}",
            sig(cross_path),
            sig(&fast),
            sig(&reference)
        );
        fast
    }
    #[cfg(all(feature = "judge-fast", not(feature = "judge-verify")))]
    fn judge(&mut self, cross_path: &LexemePath) -> LexemePath {
        self.judge_fast(cross_path)
    }
    #[cfg(not(feature = "judge-fast"))]
    fn judge(&mut self, cross_path: &LexemePath) -> LexemePath {
        self.judge_reference(cross_path)
    }

    /// 参考实现（与 Java 回溯逐位一致），用于差分对拍与正式回落。
    ///
    /// 启用条件：`test`（差分测试需调用） / `feature = "judge-verify"`（显式保留） /
    /// `not(feature = "judge-fast")`（未启用快速版时作为正式默认）。
    /// 零分配优化版本，算法与 Java 完全同构，详见 v2 §六。
    #[cfg(any(test, feature = "judge-verify", not(feature = "judge-fast")))]
    fn judge_reference(&mut self, cross_path: &LexemePath) -> LexemePath {
        // 最优路径（打擂台，无需全排序）
        let mut best: LexemePath = LexemePath::new();
        // 可变状态对象
        let mut option = LexemePath::new();

        // 直接引用底层词元切片（LexemeRef，Copy 类型），避免 clone 整段 cross_path
        let lexemes: &[LexemeRef] = cross_path.lexemes().as_slice();

        // 常驻 scratch（跨 judge 调用复用，评审 P2-2）：首次调用时 reserve，
        // 之后每次只clear +覆写，不再分配。
        let RefScratch {
            conflict_stack,
            fp_scratch,
        } = &mut self.reference;
        conflict_stack.reserve(lexemes.len());
        fp_scratch.reserve(lexemes.len());

        // 1. 第一次贪婪前进（填充 conflict_stack，复用容量）
        Self::forward_path_reference(0, lexemes, &mut option, conflict_stack);

        // 2. 初始路径作为第一个候选（clone_from_path 复用 best 容量，纯 memcpy）
        best.clone_from_path(&option);

        // 3. 回溯探索
        while let Some(conflict_index) = conflict_stack.pop() {
            let conflict_lexeme = &lexemes[conflict_index];

            Self::back_path_reference(conflict_lexeme, &mut option);
            Self::forward_path_reference(conflict_index, lexemes, &mut option, fp_scratch);

            // 打擂台：LexemePath 的 Ord 中"更优" = Ordering::Less，保留最小的
            if best.is_empty() || option < best {
                best.clone_from_path(&option);
            }
        }

        best
    }

    /// 向前遍历，构造一个无歧义词元组合，并把冲突词元的索引压入 `conflict_indices`。
    /// 不取 `&self`：调用方已持有 `&mut self.reference`（scratch 复用），再借 `&self`
    /// 会与之冲突。
    /// `conflict_indices` 由调用方复用（`clear()` 后填满），避免每步 `Vec::new()` 的堆分配。
    #[cfg(any(test, feature = "judge-verify", not(feature = "judge-fast")))]
    fn forward_path_reference(
        start_index: usize,
        lexemes: &[LexemeRef],
        option: &mut LexemePath,
        conflict_indices: &mut Vec<usize>,
    ) {
        conflict_indices.clear();
        for (offset, l) in lexemes[start_index..].iter().enumerate() {
            let i = start_index + offset;
            if !option.add_not_cross_lexeme(Lexeme::from_ref(*l)) {
                conflict_indices.push(i);
            }
        }
    }

    /// 回滚词元链，直到它能够接受指定的词元
    #[cfg(any(test, feature = "judge-verify", not(feature = "judge-fast")))]
    fn back_path_reference(lexeme: &LexemeRef, option: &mut LexemePath) {
        while option.check_cross(&Lexeme::from_ref(*lexeme)) {
            option.remove_tail();
        }
    }

    /// 快速实现（链模型重写，v3 核心）。正式默认（`judge-fast`）与差分对拍（`test`）共用。
    ///
    /// 算法（性质 1–3，详见 v3 §二/§三）：
    /// - 跳表 `next`：二分预计算每个词元的"链后继"（第一个 begin≥end 者），把 `forward_path`
    ///   退化为 O(1) 跳转；
    /// - 后缀记忆化 `payl/sz/xw/pw/pend`：沿 `next` 拓扑序（索引降序）一遍填充，任意 `chain(i)`
    ///   指标可 O(1) 合成；
    /// - 前缀聚合 `pl/ps/pxw/ppw`：沿初始链 `chain(0)` 一遍填充；
    /// - 打擂台严格复刻参考实现：初始候选 = chain(0)，冲突按索引降序（等价 LIFO pop）逐个比较，
    ///   严格小于才更新（平局保留先者）；
    /// - 终局一次 O(n) 重建，消灭两个隐性 O(n²)。
    ///
    /// 与 `judge_reference` 在 10⁵ 随机 + corpus 全量差分 0 分歧（见差分测试）。
    #[cfg(any(test, feature = "judge-fast"))]
    fn judge_fast(&mut self, cross_path: &LexemePath) -> LexemePath {
        let lexemes: &[LexemeRef] = cross_path.lexemes().as_slice();
        let n = lexemes.len();

        // 常驻 scratch（评审 P2-2）：原实现此处新建 15 个 Vec，每次 judge 调用约
        // 15 次堆分配；现全部改为跨调用复用。`reset_scratch` 的 `clear()+resize()`
        // 保证不残留上一次调用的陈旧值（`Vec::resize` 在 new_len<=len 时不覆写已有元素）。
        let FastScratch {
            begins,
            next,
            payl,
            sz,
            xw,
            pw,
            pend,
            chain0,
            in_chain0,
            pl,
            ps,
            pxw,
            ppw,
            chain0_end,
            chosen,
        } = &mut self.fast;

        // [1] begins 数组（跳表二分用）+ 跳表 next
        reset_scratch(begins, n, 0);
        for (i, l) in lexemes.iter().enumerate() {
            begins[i] = l.begin();
        }
        // next 的哨兵值是 n（表示"无后继"）
        reset_scratch(next, n, n);
        for (i, l) in lexemes.iter().enumerate() {
            // 页内坐标（与 begins 同坐标系）：end = begin + length，
            // **不用** end_position()（= offset + begin + length，全局坐标）——
            // 第 2 页起 offset > 0，混比会使 next 恒为哨兵、链模型退化
            // （2026-10-12 长文差分发现，见设计决策）
            let target = l.begin() + l.length();
            let lo = i + 1;
            if lo < n {
                let pos = begins[lo..].partition_point(|&b| b < target);
                let j = lo + pos;
                if j < n {
                    next[i] = j;
                }
            }
        }

        // [2] 后缀记忆化（索引降序，沿 next 拓扑序）
        //下标 n 是"空后缀"哨兵：payl/sz/pw/pend 为 0，xw 为 1（乘法单位元）
        reset_scratch(payl, n + 1, 0);
        reset_scratch(sz, n + 1, 0);
        reset_scratch(xw, n + 1, 1);
        reset_scratch(pw, n + 1, 0);
        reset_scratch(pend, n + 1, 0);
        for i in (0..n).rev() {
            let nxt = next[i];
            let len = lexemes[i].length() as i64;
            payl[i] = len + payl[nxt];
            sz[i] = 1 + sz[nxt];
            xw[i] = sat_mul(len, xw[nxt]);
            pw[i] = len + pw[nxt] + payl[nxt];
            pend[i] = if nxt < n {
                pend[nxt]
            } else {
                // 页内坐标（见上 `target` 的坐标纪律）
                lexemes[i].begin() + lexemes[i].length()
            };
        }

        // [3] 初始链 chain(0) + 前缀聚合
        chain0.clear();
        let mut idx = 0usize;
        while idx < n {
            chain0.push(idx);
            idx = next[idx];
        }
        let l = chain0.len();
        reset_scratch(in_chain0, n, false);
        for &c in chain0.iter() {
            in_chain0[c] = true;
        }
        // 下标 0 是空前缀哨兵：pl/ps/pw 为 0，pxw 为 1（乘法单位元）
        reset_scratch(pl, l + 1, 0);
        reset_scratch(ps, l + 1, 0);
        reset_scratch(pxw, l + 1, 1);
        reset_scratch(ppw, l + 1, 0);
        chain0_end.clear();
        for t in 1..=l {
            let w = chain0[t - 1];
            let len = lexemes[w].length() as i64;
            pl[t] = pl[t - 1] + len;
            ps[t] = ps[t - 1] + 1;
            pxw[t] = sat_mul(pxw[t - 1], len);
            ppw[t] = ppw[t - 1] + (t as i64) * len;
            // 页内坐标（见上 `target` 的坐标纪律）：end = begin + length
            chain0_end.push(lexemes[w].begin() + lexemes[w].length());
        }
        let base_begin = lexemes[chain0[0]].begin();

        // [4]+[5] 打擂台：初始候选 = chain(0) 整体；冲突按索引降序（等价 LIFO pop）。
        //
        // **Case B（§2.4 原设想的"命中即整次回退 judge_reference"安全机制）此处刻意不实现运行时回退**：
        // 链模型在 Case B 形态下与参考实现候选完全一致，回退分支纯属死代码。论证浓缩版 ——
        //   Case B 指冲突 ci 完全落在当前候选 span 左侧（ci.end ≤ path_begin）。
        //   · 当此前缀长度 t_i = 0（ci 之前无任何 chain(0) 元素）：下方 pb 取 `lexemes[ci].begin()`，
        //     候选 = chain(ci)（用 payl[ci]/sz[ci]/pend[ci]/xw[ci]/pw[ci] 合成），恰等于参考实现在
        //     同等形态下的候选（back_path 一路弹空、forward 从 ci 续接 = chain(ci)）。
        //   · 当 t_i > 0：path_begin = base_begin = lexemes[0].begin()（全局最小 begin）；而 ci 必与
        //     chain(0) 交叉 ⇒ ci.end > base_begin = path_begin，故 Case B 条件在 t_i>0 下**不可达**。
        //   经验证据互为印证：4×10⁶ 随机差分（含 profile 0 专门轰炸 t_i=0 分支）+ 8 语料集全量
        //   逐 cross_path 双跑 + 13 MD5 回归全部 0 分歧。原 §2.4 的回退承诺属过虑，已据实更新文档。
        let mut best = PathMetrics {
            payload: pl[l],
            size: ps[l],
            path_begin: base_begin as i64,
            path_end: chain0_end[l - 1] as i64,
            x: pxw[l],
            p: ppw[l],
        };
        let mut best_ci: usize = n; // 哨兵：表示"初始链整体"（终局无后缀）
        let mut best_t: usize = l;

        // 双指针：chain0 中 end_position ≤ ci.begin() 的前缀长度（ci 降序时 ptr 单调不增）
        let mut ptr = l;
        for ci in (0..n).rev() {
            if in_chain0[ci] {
                continue;
            }
            while ptr > 0 && chain0_end[ptr - 1] > lexemes[ci].begin() {
                ptr -= 1;
            }
            let t_i = ptr;
            let pb = if t_i > 0 {
                base_begin
            } else {
                lexemes[ci].begin()
            } as i64;
            let cand = PathMetrics {
                payload: pl[t_i] + payl[ci],
                size: ps[t_i] + sz[ci] as i64,
                path_begin: pb,
                path_end: pend[ci] as i64,
                x: sat_mul(pxw[t_i], xw[ci]),
                p: ppw[t_i] + (t_i as i64) * payl[ci] + pw[ci],
            };
            if cmp_metrics(&cand, &best) == Ordering::Less {
                best = cand;
                best_ci = ci;
                best_t = t_i;
            }
        }

        // [6] 终局一次重建：前缀 chain0[0..best_t] ++ chain(best_ci)
        chosen.clear();
        chosen.extend_from_slice(&chain0[0..best_t]);
        if best_ci != n {
            let mut c = best_ci;
            while c < n {
                chosen.push(c);
                c = next[c];
            }
        }
        LexemePath::from_chain_indices(lexemes, chosen)
    }
}

#[cfg(test)]
mod judge_differential_tests {
    use super::*;
    use crate::core::lexeme::Lexeme;

    /// 确定性 LCG，保证可复现（不引入 rand 依赖）。
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0
        }
        fn range(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    /// 按 profile 随机生成 cross_path，覆盖多种形态：
    /// - 0 密集重叠 + 链首长词嵌套（Case B 前兆，`t_i=0` 分支）
    /// - 1 同 begin 聚簇（payload/size/path_* 大量相等 → 逼出规则 5/6 的 x/p 比较与平局保序）
    /// - 2 长连通链 + 短词（词元数多 → x_weight 连乘饱和到 i32::MAX 的边界）
    /// - 3 长连通链 + 长词（多冲突 + 深度回溯 + 饱和）
    /// - 4 极小规模 / 全同词元（n≤3 边界）
    /// - 5 中等密度步进 begin
    ///
    /// 规模约束（length ≤ 32、词元数 ≤ 300）刻意保持在真实语料可达域内：参考实现的
    /// `p_weight` 是 i32 求和，若刻意造出溢出用例只会得到"超出真实域"的假阳性分歧。
    /// 上界下 `p_weight ≤ Σ(i+1)·32 ≈ 1.44e6，远小于 `i32::MAX`，故差分结论有效。
    /// 注：随机差分主生成器保持 n≤300 以控制 `cargo test` 耗时；更大的 cross_path（可达真实
    /// 观测最大 1976）由 `differential_judge_fast_large_n`（显式 --ignored）与 8 语料集 judge-verify
    /// 全量双跑共同覆盖（见 v3 §二/§六）。
    fn gen_case(rng: &mut Lcg) -> LexemePath {
        let mut cp = LexemePath::new();
        match rng.range(10) {
            0..=2 => {
                let n = 1 + rng.range(45) as usize;
                for _ in 0..n {
                    let (begin, length) = if rng.range(4) == 0 {
                        (0, 5 + rng.range(20) as i32)
                    } else {
                        (rng.range(25) as i32, 1 + rng.range(18) as i32)
                    };
                    cp.add_cross_lexeme(Lexeme::new(0, begin, length, Lexeme::TYPE_CNWORD));
                }
            }
            3..=4 => {
                let n = 2 + rng.range(30) as usize;
                let anchors = [0i32, 4, 9];
                for _ in 0..n {
                    let begin = anchors[rng.range(3) as usize];
                    let length = 1 + rng.range(12) as i32;
                    cp.add_cross_lexeme(Lexeme::new(0, begin, length, Lexeme::TYPE_CNWORD));
                }
            }
            p @ (5 | 6) => {
                // 链式推进：新词元 begin 落在 [path_begin, path_end) 内，必然 check_cross 通过，
                // 从而可靠构造出"整体连通"的长 cross_path（真实 judge 输入的形态）。
                let target = 60 + rng.range(240) as usize;
                let max_len = if p == 5 { 4 } else { 32 };
                let mut tries = 0usize;
                while cp.size() < target && tries < target * 4 {
                    tries += 1;
                    let length = 1 + rng.range(max_len) as i32;
                    let begin = if cp.is_empty() {
                        0
                    } else {
                        let span = (cp.path_end() - cp.path_begin()).max(1) as u64;
                        cp.path_end() - 1 - rng.range(span.min(6)) as i32
                    };
                    cp.add_cross_lexeme(Lexeme::new(0, begin, length, Lexeme::TYPE_CNWORD));
                }
            }
            7 => {
                let n = 1 + rng.range(3) as usize;
                let length = 1 + rng.range(5) as i32;
                for _ in 0..n {
                    cp.add_cross_lexeme(Lexeme::new(0, 0, length, Lexeme::TYPE_CNWORD));
                }
            }
            _ => {
                let n = 5 + rng.range(35) as usize;
                for _ in 0..n {
                    let begin = rng.range(12) as i32 * (1 + rng.range(2) as i32);
                    let length = 1 + rng.range(9) as i32;
                    cp.add_cross_lexeme(Lexeme::new(0, begin, length, Lexeme::TYPE_CNWORD));
                }
            }
        }
        cp
    }

    /// 专门构造大 cross_path（词元数 ~200..2000）以覆盖随机差分主生成器（n≤300）未触及的
    /// (300, 1976] 形态区间。仅由 `differential_judge_fast_large_n` 使用——因 `judge_reference`
    /// 为 O(n²)，大 n 下每例昂贵，故该测试限定用例数并标记为 ignored 显式运行。
    fn gen_large_case(rng: &mut Lcg) -> LexemePath {
        let mut cp = LexemePath::new();
        let target = 200 + rng.range(1800) as usize; // 200..2000
        let max_len = 1 + rng.range(32) as i32;
        let mut tries = 0usize;
        while cp.size() < target && tries < target * 4 {
            tries += 1;
            let length = 1 + rng.range(max_len as u64) as i32;
            let begin = if cp.is_empty() {
                0
            } else {
                let span = (cp.path_end() - cp.path_begin()).max(1) as u64;
                cp.path_end() - 1 - rng.range(span.min(6)) as i32
            };
            cp.add_cross_lexeme(Lexeme::new(0, begin, length, Lexeme::TYPE_CNWORD));
        }
        cp
    }

    /// 差分主体：跑 `cases` 个随机用例，逐位比较两实现输出。
    fn run_differential(seed: u64, cases: usize) {
        let mut arb = IKArbitrator::new();
        let mut rng = Lcg(seed);
        for idx in 0..cases {
            let cp = gen_case(&mut rng);
            if cp.is_empty() {
                continue;
            }
            let a = arb.judge_reference(&cp);
            let b = arb.judge_fast(&cp);
            assert_eq!(
                sig(&a),
                sig(&b),
                "judge_reference 与 judge_fast 在 seed={} 用例 {} 上分歧：\ninput={:?}\nref={:?}\nfast={:?}",
                seed,
                idx,
                sig(&cp),
                sig(&a),
                sig(&b)
            );
        }
    }

    /// 把路径转成 (begin, length) 序列，用于逐位比较词元集合与顺序。
    fn sig(p: &LexemePath) -> Vec<(i32, i32)> {
        p.lexemes()
            .as_slice()
            .iter()
            .map(|l| (l.begin(), l.length()))
            .collect()
    }

    /// 常规护栏：10⁵ 随机用例（多形态 profile），随普通 `cargo test` 一起跑。
    #[test]
    fn differential_judge_reference_vs_fast() {
        run_differential(0x1234_5678, 100_000);
    }

    /// 重压差分：10⁶ 用例 × 4 个独立 seed（合计 4×10⁶），显式运行以免拖慢常规测试：
    /// `cargo test --release --lib differential_judge_fast_heavy -- --ignored --nocapture`
    #[test]
    #[ignore = "重压用例（数分钟），显式 --ignored 运行"]
    fn differential_judge_fast_heavy() {
        for seed in [0x1234_5678u64, 0xDEAD_BEEF, 0x0BAD_C0DE, 0x5EED_5EED] {
            run_differential(seed, 1_000_000);
        }
    }

    /// 大 cross_path 专项差分（词元数 ~200..2000）：覆盖随机差分主生成器 n≤300 未触及的
    /// (300, 1976] 形态区间。`judge_reference` 为 O(n²)，故用例数受限并标记 ignored，
    /// 显式运行：`cargo test --release --lib differential_judge_fast_large_n -- --ignored`
    #[test]
    #[ignore = "大 cross_path 专项（O(n²) reference，显式 --ignored 运行）"]
    fn differential_judge_fast_large_n() {
        let mut arb = IKArbitrator::new();
        let mut rng = Lcg(0xC0FFEE);
        let cases = 20_000usize;
        for idx in 0..cases {
            let cp = gen_large_case(&mut rng);
            if cp.is_empty() {
                continue;
            }
            let a = arb.judge_reference(&cp);
            let b = arb.judge_fast(&cp);
            assert_eq!(
                sig(&a),
                sig(&b),
                "large-n 差分分歧 seed=0xC0FFEE 用例 {}：\ninput={:?}\nref={:?}\nfast={:?}",
                idx,
                sig(&cp),
                sig(&a),
                sig(&b)
            );
        }
    }

    /// 确定性小用例锁定：覆盖链首嵌套（t_i=0）+ 多重冲突 + 平局，作为可读的回归护栏。
    #[test]
    fn differential_judge_fast_locked_small_cases() {
        let mut arb = IKArbitrator::new();
        // 手工构造：begin 同位的多个长度 + 密集短词，强制产生冲突候选与平局形态。
        // 【修复】原实现把 `LexemePath::new()` 放在循环内，每次只塞 1 个词元，
        // `judge` 因 `cp.size() == 1` 直接短路返回，从未真正裁决。改为循环外累积，
        // 让单个 cp 收齐全部交叉词元，使 `size() > 1`，judge 才会真正运行。
        let cases: &[(i32, i32)] = &[
            (0, 10),
            (0, 5),
            (0, 3),
            (2, 4),
            (3, 6),
            (8, 2),
            (7, 1),
            (12, 5),
            (12, 3),
            (17, 2),
            (16, 4),
            (20, 1),
            (21, 3),
            (22, 2),
        ];
        let mut cp = LexemePath::new();
        for (begin, length) in cases {
            cp.add_cross_lexeme(Lexeme::new(0, *begin, *length, Lexeme::TYPE_CNWORD));
        }
        // 护栏：确保 judge 真正进入多词元裁决路径，而非短路返回。
        assert!(
            cp.size() > 1,
            "锁定用例必须构造多词元交叉路径才会触发 judge"
        );

        let a = arb.judge_reference(&cp);
        let b = arb.judge_fast(&cp);
        assert_eq!(
            sig(&a),
            sig(&b),
            "锁定用例分歧：\ninput={:?}\nref={:?}\nfast={:?}",
            sig(&cp),
            sig(&a),
            sig(&b)
        );
    }

    /// **跨页坐标纪律回归**（2026-10-12 长文差分发现的 Bug 类）：
    /// `LexemePath` / `judge_fast` 曾误用全局坐标 `end_position()`
    /// （= `offset + begin + length`）参与页内比较——第 1 页 `offset == 0` 时
    /// 恰好相等，Bug 完全隐匿；第 2 页起（`buff_offset > 0`，即输入 > 4096
    /// 字符触发换页后）页内 begin 与全局 end 混比，导致路径构造与链模型退化、
    /// 跨页词元被成片丢弃。本测试以**同一份页内词元序列 + 不同 offset** 驱动
    /// 两实现，锁定：
    /// ① 裁决结果（sig）与 offset 无关（页内坐标纪律）；
    /// ② 与参考实现在各 offset 下逐位一致。
    ///
    /// 背景：既有差分生成器全部使用 `Lexeme::new(0, …)`，结构上不可能暴露此类
    /// Bug；9133 行对账语料最长 646 字符，同样不可达。见
    /// 设计决策与验证层说明。
    #[test]
    fn differential_judge_fast_page_offset_discipline() {
        // 页内词元序列：与 locked_small_cases 同形态（密集冲突 + 同位多长度）
        let cases: &[(i32, i32)] = &[
            (0, 10),
            (0, 5),
            (0, 3),
            (2, 4),
            (3, 6),
            (8, 2),
            (7, 1),
            (12, 5),
            (12, 3),
            (17, 2),
            (16, 4),
            (20, 1),
            (21, 3),
            (22, 2),
        ];
        // offset 扫描：0（第 1 页，历史唯一覆盖）+ 4096/8192/65536（第 2 页及以后，
        // 对应 >4096 字符输入换页后的真实页内坐标形态）
        let offsets: &[i32] = &[0, 4096, 8192, 65536];
        let mut reference_sig: Option<Vec<(i32, i32)>> = None;
        for &offset in offsets {
            let mut cp = LexemePath::new();
            for (begin, length) in cases {
                cp.add_cross_lexeme(Lexeme::new(offset, *begin, *length, Lexeme::TYPE_CNWORD));
            }
            assert!(cp.size() > 1, "offset={offset} 用例必须多词元");

            let mut arb = IKArbitrator::new();
            let a = arb.judge_reference(&cp);
            let b = arb.judge_fast(&cp);
            assert_eq!(
                sig(&a),
                sig(&b),
                "offset={offset} 下两实现分歧：\ninput={:?}\nref={:?}\nfast={:?}",
                sig(&cp),
                sig(&a),
                sig(&b)
            );
            // 裁决结果必须与 offset 无关（纯页内坐标纪律）
            match &reference_sig {
                None => reference_sig = Some(sig(&b)),
                Some(base) => assert_eq!(
                    base,
                    &sig(&b),
                    "offset={offset} 的裁决结果与 offset=0 不同——坐标纪律被破坏？"
                ),
            }
        }
        // 终局 sanity：裁决结果非空（防止「全丢弃」形态蒙混过关）
        assert!(
            reference_sig.map(|s| !s.is_empty()).unwrap_or(false),
            "裁决结果不应为空"
        );
    }
}
