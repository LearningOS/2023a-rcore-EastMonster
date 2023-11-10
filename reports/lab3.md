# LAB 3: 进程创建和 stride 调度算法

### 功能总结
- 通过使用 `TaskControlBlock::new()` 方法建立一个新的 TCB, 并建立父进程与子进程之间的联系, 实现了 `sys_spawn` 系统调用;  
- 通过在 `fetch` 处找到 Ready 队列中 `stride` 最小的任务并取出作为返回值的方式完成了 stride 调度算法的实现.  

### 简答题

1. 实际情况是轮到 p1 执行吗？为什么？  

    不是, 因为 `stride` 是无符号数, 255 + 10 上溢得到 5, 导致接下来的数次调度仍会让 p2 运行.  

2. 为什么？尝试简单说明（不要求严格证明）。  

    以最简单的两个任务的情况为例.  
    假设 $\text{Prio}_1=a, \text{Prio}_2=b ~(a = nb ≥ 2, n \in \mathbb{Z^+})$, 则 $\text{Pass}_2= n\text{Pass}_1$. 假设进程 2 先运行, 则此时两 stride 差值最大, 为 $\text{Pass}_2 = \frac{\text{BigStride}}{\text{Prio}_2} \le \frac{\text{BigStride}}{2}$, 所以结论成立.  

3. 已知以上结论，考虑溢出的情况下，可以为 Stride 设计特别的比较器，让 BinaryHeap<Stride> 的 pop 方法能返回真正最小的 Stride。补全下列代码中的 partial_cmp 函数，假设两个 Stride 永远不会相等。  
    ```rust
    impl PartialOrd for Stride {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            let self_is_bigger = if self.0 > other.0 {
                true
            } else {
                false
            }
            let diff = if self_is_bigger {
                self.0 - other.0
            } else {
                other.0 - self.0
            }
            if diff <= BIG_STRIDE / 2 {
                if self_is_bigger {
                    Some(Ordering::Greater)
                } else {
                    Some(Ordering::Less)
                }
            } else {
                if self_is_bigger {
                    Some(Ordering::Less)
                } else {
                    Some(Ordering::Greater)
                }
            }
        }
    }
    ```

### 荣誉准则
1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与 以下各位 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：
    > 无

2. 此外，我也参考了 以下资料 ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：
    > 无

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。