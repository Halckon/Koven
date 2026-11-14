"""失败出口与清理义务的状态模型（内核 MVP 第 4 步）。

两个小模型，各自穷举全部事件交错，比较候选政策：
- 模型 A：借用型子任务与失败的传播次序（之十一 J48 行为草案第 7 条）。
- 模型 B：完成式 IO 在途时的取消、执行器失败与进程终止窗口（io-resource-handoff-cleanup）。

它只检验“政策在这个抽象里是否违反不变量”，不代表任何后端或运行时的行为。
规格见 docs/proposals/failure-cleanup-model.md。
"""
from collections import deque
from dataclasses import dataclass
import sys


@dataclass
class Report:
    states: int
    violations: dict  # 违反的不变量 -> 到达它的最短事件序列
    stuck: dict  # 卡住的种类 -> 最短事件序列
    notes: dict  # 不算违反、但值得记录的终态 -> 最短事件序列


def explore(initial, events, violations, final_notes):
    """广度优先穷举。events(state) 给出 (事件名, 后继)；没有事件的状态是终态。"""
    parent = {initial: None}
    queue = deque([initial])
    report = Report(0, {}, {}, {})

    def trace(state):
        names = []
        while parent[state] is not None:
            state, name = parent[state]
            names.append(name)
        return list(reversed(names))

    while queue:
        state = queue.popleft()
        report.states += 1
        broken = violations(state)
        for name in broken:
            report.violations.setdefault(name, trace(state))
        if broken:
            continue  # 违反之后的行为没有意义，不再展开
        successors = list(events(state))
        if not successors:
            kind, name = final_notes(state)
            if name:
                getattr(report, kind).setdefault(name, trace(state))
        for name, successor in successors:
            if successor not in parent:
                parent[successor] = (state, name)
                queue.append(successor)
    return report


# ---- 模型 A：借用型子任务

UAF = "子仍存活而父帧已析构"
STALE = "子失败后父继续使用被借出的数据"
HANG = "失败待传播时卡住"


@dataclass(frozen=True)
class ScopePolicy:
    wait_all: bool  # 父失败或子失败时，先等全部子结束再让父展开
    propagate: bool  # 子失败在作用域出口处使父失败
    cancel_wakes: bool  # 失败待传播时，阻塞中的子能被唤醒并返回


def scope_model(policy, children=2):
    """状态：(父, 数据, 各子, 父是否带着待传播的失败)。

    父：body 作用域体内 | join 等待子 | after 作用域之后 | done | failed
    子：run | blocked（等一个不会自己到来的事件）| done | failed
    """
    initial = ("body", "live", ("run",) * children, False)

    def alive(kids):
        return any(kid in ("run", "blocked") for kid in kids)

    def events(state):
        par, data, kids, pending = state
        failing = pending or "failed" in kids
        for index, kid in enumerate(kids):
            def with_kid(value):
                return kids[:index] + (value,) + kids[index + 1:]
            if kid == "run":
                for value in ("done", "failed", "blocked"):
                    yield f"子{index}:{value}", (par, data, with_kid(value), pending)
            elif kid == "blocked" and failing and policy.cancel_wakes:
                yield f"子{index}:被取消唤醒", (par, data, with_kid("done"), pending)
        if par == "body":
            yield "父:到达作用域出口", ("join", data, kids, pending)
            if not pending:
                yield "父:在作用域体内失败", ("join", data, kids, True)
        elif par == "join":
            must_fail = pending or (policy.propagate and "failed" in kids)
            if must_fail and (not policy.wait_all or not alive(kids)):
                yield "父:展开并析构数据", ("failed", "freed", kids, pending)
            elif not must_fail and not alive(kids):
                yield "父:离开作用域", ("after", data, kids, pending)
        elif par == "after":
            yield "父:使用数据后正常结束", ("done", "freed", kids, pending)

    def violations(state):
        par, data, kids, _ = state
        found = []
        if data == "freed" and alive(kids):
            found.append(UAF)
        if par == "after" and "failed" in kids:
            found.append(STALE)
        return found

    def final_notes(state):
        par, _, kids, pending = state
        if par in ("done", "failed"):
            return "notes", ""
        return "stuck", HANG if (pending or "failed" in kids) else ""

    return explore(initial, events, violations, final_notes)


# ---- 模型 B：在途 IO

WRITE_AFTER_FREE = "外部仍可访问时缓冲区已释放"
UNDISCHARGED = "执行单元已结束而清理义务未履行"
LEAK = "缓冲区被永久保留（泄漏）"


@dataclass(frozen=True)
class IoPolicy:
    cancel: str  # early：取消请求一发出任务就拿回缓冲区 | wait：等到终结证据
    on_executor_failure: str  # destroy_all | leak_frame | partial | terminate | partial_or_terminate
    abort_freezes: bool  # 请求终止之后、终止生效之前，不再释放任何资源
    frame_storage: str = "heap"  # heap：帧独立分配 | parent：帧位于父的存储里，父展开即释放


def io_model(policy):
    """状态：(任务, 缓冲区, 守卫, 请求, 执行器, 进程)。

    任务：run | susp | resumed（请求之后再次运行）| done | destroyed | leaked
    缓冲区：task 在帧内 | retained 帧已销毁但保留 | freed
    守卫：held | released（帧内另一项清理义务，例如释放锁）
    请求：none | inflight（外部可能访问缓冲区）| terminal（已有终结证据）
    """
    initial = ("run", "task", "held", "none", "alive", "alive")

    def on_failure(state):
        task, buf, guard, req, _, proc = state
        action = policy.on_executor_failure
        if action == "partial_or_terminate":
            action = "partial" if policy.frame_storage == "heap" else "terminate"
        if action == "destroy_all":
            return ("destroyed", "freed", "released", req, "failed", proc)
        if action == "leak_frame":
            return ("leaked", buf, guard, req, "failed", proc)
        if action == "partial":
            kept = "retained" if req == "inflight" else "freed"
            return ("destroyed", kept, "released", req, "failed", proc)
        return (task, buf, guard, req, "failed", "aborting")

    def events(state):
        task, buf, guard, req, executor, proc = state
        if proc == "dead":
            return
        if proc == "aborting":
            yield "进程:终止生效", (task, buf, guard, req, executor, "dead")
            if not policy.abort_freezes and task == "susp":
                yield "终止窗口内继续展开", ("destroyed", "freed", "released", req, executor, proc)
        if req == "inflight":
            yield "外部:写入缓冲区", state
            yield "外部:请求终结", (task, buf, guard, "terminal", executor, proc)
        if proc != "alive":
            return
        if task == "run":
            yield "任务:提交请求并挂起", ("susp", buf, guard, "inflight", executor, proc)
        if task == "resumed":
            yield "任务:结束并析构帧", ("done", "freed", "released", req, executor, proc)
        if task == "susp" and executor == "alive":
            if req == "terminal":
                yield "执行器:凭终结证据恢复任务", ("resumed", buf, guard, req, executor, proc)
            elif policy.cancel == "early":
                yield "取消:任务提前返回", ("resumed", buf, guard, req, executor, proc)
            yield "执行器:失败", on_failure(state)
        if buf == "retained":
            if req == "terminal" and executor == "alive":
                yield "回收保留的缓冲区", (task, "freed", guard, req, executor, proc)
            if policy.frame_storage == "parent":
                yield "父:展开并释放帧所在的存储", (task, "freed", guard, req, executor, proc)

    def violations(state):
        task, buf, guard, req, _, proc = state
        found = []
        if buf == "freed" and req == "inflight" and proc != "dead":
            found.append(WRITE_AFTER_FREE)
        if guard == "held" and task == "leaked":
            found.append(UNDISCHARGED)
        return found

    def final_notes(state):
        _, buf, _, _, _, proc = state
        return "notes", LEAK if (buf == "retained" and proc != "dead") else ""

    return explore(initial, events, violations, final_notes)


SCOPE_POLICIES = {
    "立即展开": ScopePolicy(wait_all=False, propagate=True, cancel_wakes=False),
    "等齐但不传播": ScopePolicy(wait_all=True, propagate=False, cancel_wakes=False),
    "等齐并传播": ScopePolicy(wait_all=True, propagate=True, cancel_wakes=False),
    "等齐、传播、取消可唤醒": ScopePolicy(wait_all=True, propagate=True, cancel_wakes=True),
}

IO_POLICIES = {
    "取消即返回": IoPolicy("early", "partial", True),
    "随帧销毁": IoPolicy("wait", "destroy_all", True),
    "整帧保留": IoPolicy("wait", "leak_frame", True),
    "部分销毁并保留缓冲区": IoPolicy("wait", "partial", True),
    "部分销毁，帧在父存储里": IoPolicy("wait", "partial", True, "parent"),
    "终止进程但窗口内继续展开": IoPolicy("wait", "terminate", False),
    "保留优先，不能保留则终止": IoPolicy("wait", "partial_or_terminate", True),
    "保留优先，不能保留则终止（帧在父存储里）": IoPolicy("wait", "partial_or_terminate", True, "parent"),
}


def main():
    for title, model, policies in (("模型 A：借用型子任务", scope_model, SCOPE_POLICIES),
                                   ("模型 B：在途 IO", io_model, IO_POLICIES)):
        print(title)
        for name, policy in policies.items():
            report = model(policy)
            print(f"  {name}（{report.states} 个状态）")
            for kind, found in (("违反", report.violations), ("卡住", report.stuck), ("记录", report.notes)):
                for what, trace in found.items():
                    print(f"    {kind}：{what} ← {' → '.join(trace)}")
            if not (report.violations or report.stuck or report.notes):
                print("    没有违反")
    return 0


if __name__ == "__main__":
    sys.exit(main())
