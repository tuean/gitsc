//! 提交图（commit graph）的泳道布局。
//!
//! 输入是按 `git log --date-order` 排好序的提交，输出每一行要画的竖线、节点和
//! 汇合线。算法与 IDEA / GitKraken 的做法一致：每条泳道记录“下一个应当出现的
//! 提交”，遇到提交就占住期望它的那条道，父提交沿该道继续，其余父提交另开道。

use crate::log::Commit;

/// 可用泳道颜色数（渲染层按它取色）。
pub const PALETTE: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub color: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphRow {
    /// 本提交所在的泳道。
    pub lane: usize,
    pub color: usize,
    /// 贯穿整行的泳道（上方和下方都有后续提交）。
    pub pass: Vec<(usize, usize)>, // (lane, color)
    /// 从节点向下连到父提交的线。
    pub edges: Vec<Edge>,
    /// 上面是否有一条线进入本节点。
    pub from_top: bool,
    /// 这一行实际用到的泳道数。
    pub lane_count: usize,
}

impl GraphRow {
    pub fn width_lanes(&self) -> usize {
        self.pass
            .iter()
            .map(|(l, _)| *l + 1)
            .chain(self.edges.iter().map(|e| e.from.max(e.to) + 1))
            .chain(std::iter::once(self.lane + 1))
            .max()
            .unwrap_or(1)
    }
}

/// 计算整段历史的泳道布局。返回与 `commits` 等长的行；整张图的泳道数取
/// 各行最大值（渲染时所有行用同一宽度，避免节点左右跳动）。
pub fn layout(commits: &[Commit]) -> (Vec<GraphRow>, usize) {
    let mut lanes: Vec<Option<String>> = Vec::new();
    let mut colors: Vec<usize> = Vec::new();
    let mut next_color = 0usize;
    let mut rows: Vec<GraphRow> = Vec::with_capacity(commits.len());
    let mut max_lanes = 1usize;

    for commit in commits {
        let top: Vec<Option<String>> = lanes.clone();

        // 1. 找到（可能有多条）等待这个提交的泳道，保留最左一条，其余收掉。
        let mut node_lane = None;
        for (i, slot) in lanes.iter_mut().enumerate() {
            if slot.as_deref() == Some(commit.oid.as_str()) {
                if node_lane.is_none() {
                    node_lane = Some(i);
                } else {
                    *slot = None;
                }
            }
        }

        // 2. 没有任何泳道等它 —— 这是一个分支头，占用一条空道（没有就新开）。
        let node_lane = match node_lane {
            Some(i) => i,
            None => {
                let i = free_lane(&mut lanes, &mut colors);
                colors[i] = next_color;
                next_color = (next_color + 1) % PALETTE;
                i
            }
        };
        let node_color = colors[node_lane];

        // 3. 父提交：第一个留在本道，其余并入已有泳道或新开一条。
        lanes[node_lane] = None;
        let mut edges = Vec::with_capacity(commit.parents.len());
        for parent in &commit.parents {
            // 已经有别的泳道在等这个父提交 → 并过去（这样 merge 才收得回来）。
            // 否则第一个父留在本道，其余父新开一条。
            let existing = lanes
                .iter()
                .enumerate()
                .find(|(i, l)| *i != node_lane && l.as_deref() == Some(parent.as_str()))
                .map(|(i, _)| i);
            let target = match existing {
                Some(i) => i,
                None if edges.is_empty() => node_lane,
                None => {
                    let i = free_lane(&mut lanes, &mut colors);
                    colors[i] = next_color;
                    next_color = (next_color + 1) % PALETTE;
                    i
                }
            };
            lanes[target] = Some(parent.clone());
            edges.push(Edge {
                from: node_lane,
                to: target,
                color: colors[target],
            });
        }

        // 4. 贯穿线：上方和下方都还占用、且不是本节点的泳道。
        let mut pass = Vec::new();
        for (i, slot) in top.iter().enumerate() {
            if i == node_lane || slot.is_none() {
                continue;
            }
            if lanes.get(i).is_some_and(|l| l.is_some()) {
                pass.push((i, colors[i]));
            }
        }

        let lane_count = top
            .len()
            .max(lanes.len())
            .max(node_lane + 1)
            .max(1);
        max_lanes = max_lanes.max(lane_count);
        rows.push(GraphRow {
            lane: node_lane,
            color: node_color,
            pass,
            edges,
            from_top: top.get(node_lane).is_some_and(|l| l.is_some()),
            lane_count,
        });
    }

    (rows, max_lanes)
}

fn free_lane(lanes: &mut Vec<Option<String>>, colors: &mut Vec<usize>) -> usize {
    if let Some(i) = lanes.iter().position(|l| l.is_none()) {
        i
    } else {
        lanes.push(None);
        colors.push(0);
        lanes.len() - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::CommitRef;

    fn commit(oid: &str, parents: &[&str]) -> Commit {
        Commit {
            oid: oid.into(),
            parents: parents.iter().map(|s| s.to_string()).collect(),
            author: "a".into(),
            email: "a@b.c".into(),
            time: 0,
            refs: Vec::<CommitRef>::new(),
            summary: oid.into(),
        }
    }

    #[test]
    fn linear_history_stays_in_one_lane() {
        let commits = vec![commit("c", &["b"]), commit("b", &["a"]), commit("a", &[])];
        let (rows, lanes) = layout(&commits);
        assert_eq!(lanes, 1);
        assert_eq!(rows[0].lane, 0);
        assert_eq!(rows[0].edges.len(), 1);
        assert_eq!(rows[0].edges[0].to, 0);
        // 第一个提交是分支头，上方没有线进来；第二个提交才从上方接上。
        assert!(!rows[0].from_top);
        assert!(rows[1].from_top);
        assert!(rows[2].edges.is_empty());
    }

    #[test]
    fn merge_creates_a_second_lane_and_merges_back() {
        //   m (parents: a, b)   a = first parent, b = side branch
        //   a (parent: base)    b (parent: base)
        //   base
        let commits = vec![
            commit("m", &["a", "b"]),
            commit("a", &["base"]),
            commit("b", &["base"]),
            commit("base", &[]),
        ];
        let (rows, lanes) = layout(&commits);
        assert_eq!(rows[0].lane, 0);
        assert_eq!(rows[0].edges.len(), 2);
        assert_eq!(rows[0].edges[0].to, 0);
        assert_eq!(rows[0].edges[1].to, 1, "第二父应占新泳道");
        assert!(lanes >= 2);
        // b 与 a 都指向 base：b 那一行连回 a 占住的 0 号道。
        assert_eq!(rows[2].lane, 1);
        assert_eq!(rows[2].edges[0].to, 0);
        // base 收敛到 0 号道，1 号道不再贯穿。
        assert_eq!(rows[3].lane, 0);
        assert!(rows[3].edges.is_empty());
    }

    #[test]
    fn two_tips_in_flight_use_separate_lanes() {
        // 两个分支头同时在窗口里，各自的父提交还没出现 → 必须占两条道。
        let commits = vec![
            commit("x", &["px"]),
            commit("y", &["py"]),
            commit("px", &[]),
            commit("py", &[]),
        ];
        let (rows, lanes) = layout(&commits);
        assert_eq!(rows[0].lane, 0);
        assert_eq!(rows[1].lane, 1);
        assert_eq!(rows[2].lane, 0);
        assert_eq!(rows[3].lane, 1);
        assert_eq!(lanes, 2);
    }

    #[test]
    fn octopus_merge_uses_three_lanes() {
        let commits = vec![
            commit("m", &["a", "b", "c"]),
            commit("a", &[]),
            commit("b", &[]),
            commit("c", &[]),
        ];
        let (rows, lanes) = layout(&commits);
        assert_eq!(rows[0].edges.len(), 3);
        assert_eq!(lanes, 3);
    }
}
