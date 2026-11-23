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


# ---- 模型 A2：单线程作用域取消、清理排空与确切错误归属（3A 验收）

WRONG_ERROR = "作用域传播了错误的失败原因（未保留确切 err_A）"
DEINIT_FAIL_SUPPRESSED = "子任务 deinit 失败被首错优先覆盖而未终止进程"


@dataclass(frozen=True)
class ScopeCancelDrainPolicy:
    wake_is_done: bool  # 唤醒阻塞子任务即视作已完成（未等 cleaning 排空）
    timeout_releases_parent: bool  # 超时假装子任务结束并释放父帧借用
    cancel_overwrites_err: bool  # 取消信号或被取消子的结果覆盖先发 err_A
    suppress_deinit_fail: bool  # 子任务 deinit 自身失败被首错优先吞掉而不终止进程


def scope_cancel_drain_model(policy):
    """状态：(父, 父借出数据, 子A, 子B, 记录首错, 父传播结果, 进程)。

    场景：丢弃 Job_A 后子 A 失败产生 err_A，子 B 处于可唤醒阻塞点。
    子 B 显式区分 blocked -> cleaning（已唤醒仍存活）-> done | deinit_failed。
    """
    initial = ("body", "live", "run", "blocked", None, None, "alive")

    def b_alive(b):
        return b in ("blocked", "cleaning")

    def any_alive(a, b):
        return a == "run" or b_alive(b)

    def events(state):
        par, data, a, b, first_err, par_err, proc = state
        if proc != "alive" or par == "failed":
            return
        if a == "run":
            yield "丢弃Job_A后子A失败(err_A)", (par, data, "failed_err_A", b, first_err or "err_A", par_err, proc)
        if par == "body":
            yield "父:到达作用域出口(等待子任务)", ("join", data, a, b, first_err, par_err, proc)
        if a == "failed_err_A":
            if b == "blocked":
                next_err = "cancelled" if policy.cancel_overwrites_err else first_err
                yield "取消唤醒子B(进入cleaning)", (par, data, a, "cleaning", next_err, par_err, proc)
                if policy.timeout_releases_parent and par == "join":
                    yield "超时假装子B结束并释放父帧", ("failed", "freed", a, b, first_err, first_err, proc)
            elif b == "cleaning":
                yield "子B:完成清理并实际终止", (par, data, a, "done", first_err, par_err, proc)
                if policy.suppress_deinit_fail:
                    yield "子B:deinit失败但被首错覆盖", (par, data, a, "deinit_failed", first_err, par_err, proc)
                else:
                    yield "子B:deinit失败触发进程终止", ("aborted", data, a, "deinit_failed", first_err, None, "aborted")
                if policy.wake_is_done and par == "join":
                    yield "父:误把唤醒当作已结束并释放父帧", ("failed", "freed", a, b, first_err, first_err, proc)
            elif par == "join":
                yield "父:子全部终止后释放父帧并经普通失败出口传播错误", ("failed", "freed", a, b, first_err, first_err, proc)

    def violations(state):
        par, data, a, b, _, par_err, proc = state
        found = []
        if data == "freed" and any_alive(a, b) and proc == "alive":
            found.append(UAF)
        if par == "failed" and par_err != "err_A":
            found.append(WRONG_ERROR)
        if b == "deinit_failed" and proc == "alive":
            found.append(DEINIT_FAIL_SUPPRESSED)
        return found

    def final_notes(state):
        par, _, _, _, _, _, proc = state
        if proc == "aborted" or par == "failed":
            return "notes", ""
        return "stuck", HANG

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
    buffer_owner: str = "frame"  # frame：缓冲区始终属于帧 | operation：提交时移交给操作，终结后交还
    successor: bool = False  # 执行器失败后，是否有后继凭终结证据回收被保留的缓冲区


def io_model(policy):
    """状态：(任务, 缓冲区, 守卫, 请求, 执行器, 进程)。

    任务：run | susp | resumed（请求之后再次运行）| done | destroyed | leaked
    缓冲区：task 在帧内 | retained 不在帧内但保留（帧已销毁，或由操作持有）| freed
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
            return ("destroyed", buf if buf == "retained" else "freed", "released", req, "failed", proc)
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
            held = "retained" if policy.buffer_owner == "operation" else buf
            yield "任务:提交请求并挂起", ("susp", held, guard, "inflight", executor, proc)
        if task == "resumed":
            yield "任务:结束并析构帧", ("done", "freed", "released", req, executor, proc)
        if task == "susp" and executor == "alive":
            if req == "terminal":
                yield "执行器:凭终结证据恢复任务", ("resumed", "task", guard, req, executor, proc)
            elif policy.cancel == "early":
                yield "取消:任务提前返回", ("resumed", buf, guard, req, executor, proc)
            yield "执行器:失败", on_failure(state)
        if buf == "retained" and task in ("destroyed", "leaked"):
            if req == "terminal" and (executor == "alive" or policy.successor):
                yield "回收保留的缓冲区", (task, "freed", guard, req, executor, proc)
            if policy.frame_storage == "parent" and policy.buffer_owner == "frame":
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


# ---- 模型 C：多请求、证据丢失/迟到与请求代际

STALE_RECLAIM = "凭旧代际的迟到证据误释放新请求缓冲区"


@dataclass(frozen=True)
class MultiIoPolicy:
    check_generation: bool  # 回收时是否核对请求身份与代际 (id, gen)
    reclaim_without_evidence: bool  # 证据丢失后是否凭超时直接回收
    batch_reclaim_on_any: bool  # 任一请求终结时是否整批回收全部缓冲区
    evidence_can_lose: bool = False  # 完成队列溢出或执行器崩溃是否可能丢失终结证据


def multi_io_model(policy):
    """状态：(槽0状态, 槽0代际, 槽1状态, 槽0是否有来自代际1的迟到证据)。

    槽状态：inflight（外部可能访问）| terminal（已收到本代终结证据）| lost（证据丢失，外部状态未知）| freed
    """
    initial = ("inflight", 1, "inflight", False)

    def events(state):
        s0, g0, s1, stale = state
        if s0 == "inflight":
            yield f"槽0(g{g0}):收到终结证据", ("terminal", g0, s1, stale)
            if policy.evidence_can_lose:
                yield f"槽0(g{g0}):终结证据丢失", ("lost", g0, s1, stale)
        if s1 == "inflight":
            yield "槽1:收到终结证据", (s0, g0, "terminal", stale)
            if policy.evidence_can_lose:
                yield "槽1:终结证据丢失", (s0, g0, "lost", stale)
        if s0 == "terminal":
            if policy.batch_reclaim_on_any:
                ns1 = "freed_lost" if s1 == "inflight" else "freed"
                yield f"槽0(g{g0}):整批回收全部槽位", ("freed", g0, ns1, stale)
            else:
                yield f"槽0(g{g0}):按身份回收", ("freed", g0, s1, stale)
        if s1 == "terminal":
            if policy.batch_reclaim_on_any:
                ns0 = "freed_lost" if s0 == "inflight" else "freed"
                yield "槽1:整批回收全部槽位", (ns0, g0, "freed", stale)
            else:
                yield "槽1:按身份回收", (s0, g0, "freed", stale)
        if s0 == "freed" and g0 == 1:
            # 槽 0 复用为第 2 代请求，同时可能残留一条来自第 1 代的迟到/重复完成事件
            yield "槽0:复用提交g2(带g1迟到事件)", ("inflight", 2, s1, True)
        if stale and g0 == 2 and s0 == "inflight":
            if policy.check_generation:
                yield "槽0:核对代际后丢弃g1迟到证据", (s0, g0, s1, False)
            else:
                yield "槽0:未核对代际凭g1迟到证据误释放g2", ("freed_stale", g0, s1, False)
        if policy.reclaim_without_evidence:
            if s0 == "lost":
                yield f"槽0(g{g0}):无证据超时回收", ("freed_lost", g0, s1, stale)
            if s1 == "lost":
                yield "槽1:无证据超时回收", (s0, g0, "freed_lost", stale)

    def violations(state):
        s0, _, s1, _ = state
        found = []
        if s0 == "freed_stale":
            found.append(STALE_RECLAIM)
        if s0 == "freed_lost" or s1 == "freed_lost":
            found.append(WRITE_AFTER_FREE)
        return found

    def final_notes(state):
        s0, _, s1, _ = state
        return "notes", LEAK if (s0 == "lost" or s1 == "lost") else ""

    return explore(initial, events, violations, final_notes)


# ---- 模型 D：监督边界与多线程终止窗口

CORRUPT_STATE = "监督边界外观察到未恢复的共享状态"


@dataclass(frozen=True)
class SupervisionPolicy:
    isolated_state: bool  # 监督边界内外是否隔离可变状态
    supervisor_restarts: bool  # 监督者捕获子失败后恢复/重启，还是升级为进程终止
    abort_freezes_all_threads: bool  # 终止窗口内是否冻结其他线程的析构释放


def supervision_model(policy):
    """状态：(子单元, 边界外共享状态, 监督者, 同伴线程, 同伴在途缓冲区)。"""
    initial = ("run", "clean", "watching", "run", "inflight")

    def events(state):
        child, shared, sup, peer, buf = state
        if sup == "dead":
            return
        if sup == "aborting":
            yield "进程:终止生效", (child, shared, "dead", "stopped", buf)
            if not policy.abort_freezes_all_threads and peer == "run" and buf == "inflight":
                yield "终止窗口内同伴线程继续展开并释放缓冲区", (child, shared, sup, "stopped", "freed_early")
            return
        if child == "run":
            yield "子单元:正常结束", ("stopped", shared, sup, peer, buf)
            next_shared = shared if policy.isolated_state else "dirty"
            yield "子单元:失败到达监督边界", ("failed", next_shared, sup, peer, buf)
        if child == "failed" and sup == "watching":
            if policy.supervisor_restarts:
                yield "监督者:捕获失败并继续运行", ("stopped", shared, "recovered", peer, buf)
            else:
                yield "监督者:升级为进程终止", ("stopped", shared, "aborting", peer, buf)
        if peer == "run" and buf == "inflight":
            yield "同伴线程:收到终结证据并释放缓冲区", (child, shared, sup, "stopped", "freed_safe")

    def violations(state):
        _, shared, sup, _, buf = state
        found = []
        if sup == "recovered" and shared == "dirty":
            found.append(CORRUPT_STATE)
        if sup == "aborting" and buf == "freed_early":
            found.append(WRITE_AFTER_FREE)
        return found

    def final_notes(state):
        return "notes", ""

    return explore(initial, events, violations, final_notes)


# ---- 模型 E：清理进度的记录与恢复（IO 原则页 §2.4）

DOUBLE_CLEANUP = "重复清理同一资源"


@dataclass(frozen=True)
class CleanupProgressPolicy:
    persist_checkpoint: bool  # 每完成一步清理是否持久化记录进度
    idempotent_steps: bool  # 重放已完成步骤是否幂等（否则导致重复释放）
    has_successor: bool  # 主清理者中途崩溃后是否有后继接管
    atomic_checkpoint: bool = True  # 释放资源与写进度是否原子（False 表示两者之间可能崩溃）


def cleanup_progress_model(policy):
    """状态：(清理者, 资源1, 资源2, 已持久化进度)。

    清理者：primary | primary_r1_unrecorded | crashed | successor | done
    资源：held | freed | double_freed
    进度：0 未开始 | 1 已记录资源1完成 | 2 已记录资源2完成
    """
    initial = ("primary", "held", "held", 0)

    def step_r1(cleaner, r1, r2, cp):
        nr1 = "freed" if r1 == "held" else ("freed" if policy.idempotent_steps else "double_freed")
        ncp = 1 if policy.persist_checkpoint else cp
        return (cleaner, nr1, r2, ncp)

    def step_r2(cleaner, r1, r2, cp):
        nr2 = "freed" if r2 == "held" else ("freed" if policy.idempotent_steps else "double_freed")
        ncp = 2 if policy.persist_checkpoint else cp
        return ("done", r1, nr2, ncp)

    def events(state):
        cleaner, r1, r2, cp = state
        if cleaner == "done":
            return
        if cleaner == "primary":
            if r1 == "held":
                if policy.atomic_checkpoint:
                    yield "主清理者:清理资源1", step_r1("primary", r1, r2, cp)
                else:
                    yield "主清理者:释放资源1(进度未落盘)", ("primary_r1_unrecorded", "freed", r2, cp)
                yield "主清理者:在资源1前崩溃", ("crashed", r1, r2, cp)
            elif r2 == "held":
                yield "主清理者:清理资源2", step_r2("primary", r1, r2, cp)
                yield "主清理者:在资源1之后崩溃", ("crashed", r1, r2, cp)
        elif cleaner == "primary_r1_unrecorded":
            ncp = 1 if policy.persist_checkpoint else cp
            yield "主清理者:写入资源1进度", ("primary", r1, r2, ncp)
            yield "主清理者:释放资源1后写进度前崩溃", ("crashed", r1, r2, cp)
        elif cleaner == "crashed":
            if policy.has_successor:
                yield "后继:接管清理", ("successor", r1, r2, cp)
        elif cleaner == "successor":
            if cp < 1:
                yield "后继:执行资源1清理", step_r1("successor", r1, r2, 1)
            elif cp < 2:
                yield "后继:执行资源2清理", step_r2("successor", r1, r2, 2)

    def violations(state):
        cleaner, r1, r2, _ = state
        found = []
        if r1 == "double_freed" or r2 == "double_freed":
            found.append(DOUBLE_CLEANUP)
        if cleaner == "crashed" and not policy.has_successor and (r1 == "held" or r2 == "held"):
            found.append(UNDISCHARGED)
        return found

    def final_notes(state):
        return "notes", ""

    return explore(initial, events, violations, final_notes)


# ---- 模型 F：锁中毒与 channel 对端关闭（之十一 §1.2、§1.4）

LOCK_OR_CHAN_HANG = "同伴因锁未释放或 channel 未关闭而永久阻塞"


@dataclass(frozen=True)
class LockChannelPolicy:
    release_on_unwind: bool  # 展开时是否释放锁
    poison_on_unwind: bool  # 展开释放锁时是否标记为中毒
    close_channel_on_unwind: bool  # 展开时是否析构发送端并关闭 channel


def lock_channel_model(policy):
    """状态：(工作单元, 锁, 共享数据, channel, 同伴角色, 同伴状态)。

    工作单元：holding | unwound | done
    锁：locked | unlocked | poisoned
    共享数据：clean | dirty | observed_dirty
    channel：open | closed
    同伴角色：lock（等锁）| recv（等消息）
    同伴状态：waiting | done
    """
    initial = ("holding", "locked", "dirty", "open", "choose", "waiting")

    def events(state):
        worker, mutex, shared, chan, role, peer = state
        if role == "choose":
            yield "同伴:尝试加锁", (worker, mutex, shared, chan, "lock", peer)
            yield "同伴:等待接收消息", (worker, mutex, shared, chan, "recv", peer)
            return
        if worker == "holding":
            yield "工作单元:正常完成并提交", ("done", "unlocked", "clean", "closed", role, peer)
            nmutex = ("poisoned" if policy.poison_on_unwind else "unlocked") if policy.release_on_unwind else "locked"
            nchan = "closed" if policy.close_channel_on_unwind else "open"
            yield "工作单元:失败并展开至监督边界", ("unwound", nmutex, shared, nchan, role, peer)
        if peer == "waiting":
            if role == "lock":
                if mutex == "unlocked":
                    nshared = "observed_dirty" if shared == "dirty" else shared
                    yield "同伴:获得锁并读取共享数据", (worker, "unlocked", nshared, chan, role, "done")
                elif mutex == "poisoned":
                    yield "同伴:收到锁中毒结果", (worker, mutex, shared, chan, role, "done")
            elif role == "recv":
                if chan == "closed":
                    yield "同伴:收到通道关闭(null)", (worker, mutex, shared, chan, role, "done")

    def violations(state):
        _, _, shared, _, _, _ = state
        return [CORRUPT_STATE] if shared == "observed_dirty" else []

    def final_notes(state):
        _, _, _, _, role, peer = state
        if role != "choose" and peer == "waiting":
            return "stuck", LOCK_OR_CHAN_HANG
        return "notes", ""

    return explore(initial, events, violations, final_notes)


SCOPE_POLICIES = {
    "立即展开": ScopePolicy(wait_all=False, propagate=True, cancel_wakes=False),
    "等齐但不传播": ScopePolicy(wait_all=True, propagate=False, cancel_wakes=False),
    "等齐并传播": ScopePolicy(wait_all=True, propagate=True, cancel_wakes=False),
    "等齐、传播、取消可唤醒": ScopePolicy(wait_all=True, propagate=True, cancel_wakes=True),
}

SCOPE_CANCEL_DRAIN_POLICIES = {
    "唤醒即视作结束": ScopeCancelDrainPolicy(wake_is_done=True, timeout_releases_parent=False,
                                       cancel_overwrites_err=False, suppress_deinit_fail=False),
    "超时释放父帧": ScopeCancelDrainPolicy(wake_is_done=False, timeout_releases_parent=True,
                                      cancel_overwrites_err=False, suppress_deinit_fail=False),
    "取消结果覆盖首错": ScopeCancelDrainPolicy(wake_is_done=False, timeout_releases_parent=False,
                                        cancel_overwrites_err=True, suppress_deinit_fail=False),
    "首错覆盖子deinit失败": ScopeCancelDrainPolicy(wake_is_done=False, timeout_releases_parent=False,
                                            cancel_overwrites_err=False, suppress_deinit_fail=True),
    "等子实际终止、保留err_A、deinit失败终止进程": ScopeCancelDrainPolicy(
        wake_is_done=False, timeout_releases_parent=False,
        cancel_overwrites_err=False, suppress_deinit_fail=False),
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
    "操作持有缓冲区，随帧销毁其余内容": IoPolicy("wait", "destroy_all", True, "parent", "operation"),
    "操作持有缓冲区，且有后继回收": IoPolicy("wait", "destroy_all", True, "parent", "operation", True),
}

MULTI_IO_POLICIES = {
    "不核对代际": MultiIoPolicy(check_generation=False, reclaim_without_evidence=False, batch_reclaim_on_any=False),
    "任一终结即整批回收": MultiIoPolicy(check_generation=True, reclaim_without_evidence=False, batch_reclaim_on_any=True),
    "证据丢失后超时回收": MultiIoPolicy(check_generation=True, reclaim_without_evidence=True, batch_reclaim_on_any=False,
                                  evidence_can_lose=True),
    "按身份与代际逐项回收（证据不丢失）": MultiIoPolicy(check_generation=True, reclaim_without_evidence=False,
                                         batch_reclaim_on_any=False, evidence_can_lose=False),
    "按身份与代际逐项回收（证据可丢失，丢失即保留）": MultiIoPolicy(check_generation=True, reclaim_without_evidence=False,
                                                 batch_reclaim_on_any=False, evidence_can_lose=True),
}

SUPERVISION_POLICIES = {
    "边界共享可变状态": SupervisionPolicy(isolated_state=False, supervisor_restarts=True, abort_freezes_all_threads=True),
    "隔离边界但终止窗口未冻结他线程": SupervisionPolicy(isolated_state=True, supervisor_restarts=False,
                                          abort_freezes_all_threads=False),
    "隔离边界、监督恢复且全线程冻结": SupervisionPolicy(isolated_state=True, supervisor_restarts=True,
                                          abort_freezes_all_threads=True),
    "隔离边界、升级终止且全线程冻结": SupervisionPolicy(isolated_state=True, supervisor_restarts=False,
                                          abort_freezes_all_threads=True),
}

CLEANUP_PROGRESS_POLICIES = {
    "无后继恢复": CleanupProgressPolicy(persist_checkpoint=True, idempotent_steps=False, has_successor=False),
    "无进度记录且非幂等": CleanupProgressPolicy(persist_checkpoint=False, idempotent_steps=False, has_successor=True),
    "持久化进度记录（非幂等步骤）": CleanupProgressPolicy(persist_checkpoint=True, idempotent_steps=False, has_successor=True),
    "无进度记录但步骤幂等": CleanupProgressPolicy(persist_checkpoint=False, idempotent_steps=True, has_successor=True),
    "非原子写进度且非幂等": CleanupProgressPolicy(persist_checkpoint=True, idempotent_steps=False,
                                        has_successor=True, atomic_checkpoint=False),
    "非原子写进度但步骤幂等": CleanupProgressPolicy(persist_checkpoint=True, idempotent_steps=True,
                                        has_successor=True, atomic_checkpoint=False),
}

LOCK_CHANNEL_POLICIES = {
    "不展开（原地停住）": LockChannelPolicy(release_on_unwind=False, poison_on_unwind=False, close_channel_on_unwind=False),
    "展开释放并关通道但不标中毒": LockChannelPolicy(release_on_unwind=True, poison_on_unwind=False, close_channel_on_unwind=True),
    "展开释放并标中毒但不关通道": LockChannelPolicy(release_on_unwind=True, poison_on_unwind=True, close_channel_on_unwind=False),
    "展开释放、标中毒并关闭通道": LockChannelPolicy(release_on_unwind=True, poison_on_unwind=True, close_channel_on_unwind=True),
}


def main():
    for title, model, policies in (("模型 A：借用型子任务", scope_model, SCOPE_POLICIES),
                                   ("模型 A2：作用域取消排空与确切错误归属", scope_cancel_drain_model,
                                    SCOPE_CANCEL_DRAIN_POLICIES),
                                   ("模型 B：在途 IO", io_model, IO_POLICIES),
                                   ("模型 C：多请求、证据丢失与代际", multi_io_model, MULTI_IO_POLICIES),
                                   ("模型 D：监督边界与多线程终止窗口", supervision_model, SUPERVISION_POLICIES),
                                   ("模型 E：清理进度的记录与恢复", cleanup_progress_model, CLEANUP_PROGRESS_POLICIES),
                                   ("模型 F：锁中毒与 channel 对端关闭", lock_channel_model, LOCK_CHANNEL_POLICIES)):
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
