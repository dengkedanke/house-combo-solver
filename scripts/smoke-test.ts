// 冒烟测试：浏览器降级求解器（jsSolver.ts）多场景验证
// 运行：node --experimental-strip-types scripts/smoke-test.ts
// 覆盖修复点：权重偏好（分层优化）、手动输入、≥1 约束、备选方案去重、
// 大组合数不崩溃、algorithm 标记（P1-4/P0-2 判定依赖）
import { jsSolve, jsEnumerate } from '../src/utils/jsSolver.ts';

let passed = 0;
let failed = 0;

function check(name: string, cond: boolean, detail = '') {
  if (cond) {
    passed++;
    console.log(`  ✅ ${name}`);
  } else {
    failed++;
    console.log(`  ❌ ${name}${detail ? ` — ${detail}` : ''}`);
  }
}

const qty = (r: { assignments: { combinationId: string; quantity: number }[] }, id: string) =>
  r.assignments.find((a) => a.combinationId === id)?.quantity ?? 0;

console.log('① 基础求解 + 默认权重公平性');
{
  const r = jsSolve({
    houseTypes: [{ id: 't1', name: 'A', quantity: 6 }],
    combinations: [
      { id: 'c1', name: '组合1', items: [{ typeId: 't1', count: 1 }] },
      { id: 'c2', name: '组合2', items: [{ typeId: 't1', count: 1 }] },
    ],
    manualInputs: []
  });
  check('利用率最优 totalUsed=6', r.totalUsed === 6, `used=${r.totalUsed}`);
  check('默认权重保持均衡 3/3', qty(r, 'c1') === 3 && qty(r, 'c2') === 3, `x1=${qty(r, 'c1')} x2=${qty(r, 'c2')}`);
  check('algorithm 含 preview（P1-4 判定依据）', r.algorithm.includes('preview'), r.algorithm);
}

console.log('② 权重偏好优先（跳过均衡化）');
{
  const r = jsSolve({
    houseTypes: [{ id: 't1', name: 'A', quantity: 10 }],
    combinations: [
      { id: 'c1', name: '高权重', weight: 10, items: [{ typeId: 't1', count: 1 }] },
      { id: 'c2', name: '低权重', weight: 1, items: [{ typeId: 't1', count: 1 }] },
    ],
    manualInputs: []
  });
  check('高权重组合优先 x1>x2', qty(r, 'c1') > qty(r, 'c2'), `x1=${qty(r, 'c1')} x2=${qty(r, 'c2')}`);
  check('权重偏好不牺牲利用率 totalUsed=10', r.totalUsed === 10, `used=${r.totalUsed}`);
}

console.log('③ 权重不牺牲利用率（大组合）');
{
  const r = jsSolve({
    houseTypes: [{ id: 't1', name: 'A', quantity: 10 }],
    combinations: [
      { id: 'c1', name: '小组合', weight: 1, items: [{ typeId: 't1', count: 1 }] },
      { id: 'c2', name: '大组合', weight: 10, items: [{ typeId: 't1', count: 2 }] },
    ],
    manualInputs: []
  });
  check('利用率保持最优 totalUsed=10', r.totalUsed === 10, `used=${r.totalUsed}`);
  check('高权重大组合更优先 x2>=x1', qty(r, 'c2') >= qty(r, 'c1'), `x1=${qty(r, 'c1')} x2=${qty(r, 'c2')}`);
}

console.log('④ 手动输入固定数量');
{
  const r = jsSolve({
    houseTypes: [
      { id: 't1', name: 'A', quantity: 10 },
      { id: 't2', name: 'B', quantity: 10 },
    ],
    combinations: [
      { id: 'c1', name: '手动组合', items: [{ typeId: 't1', count: 1 }] },
      { id: 'c2', name: '自由组合', items: [{ typeId: 't2', count: 1 }] },
    ],
    manualInputs: [{ combinationId: 'c1', quantity: 2 }]
  });
  const a1 = r.assignments.find((a) => a.combinationId === 'c1');
  check('手动数量固定为 2', a1?.quantity === 2, `qty=${a1?.quantity}`);
  check('手动标记 isManual=true', a1?.isManual === true);
}

console.log('⑤ 数量区间（min/max 约束，替换原 ≥1）');
{
  // min=1：组合2 必须 ≥1
  const r = jsSolve({
    houseTypes: [{ id: 't1', name: 'A', quantity: 6 }],
    combinations: [
      { id: 'c1', name: '组合1', items: [{ typeId: 't1', count: 1 }], min: 0, max: 999 },
      { id: 'c2', name: '组合2', items: [{ typeId: 't1', count: 2 }], min: 1, max: 999 },
    ],
    manualInputs: [],
  });
  check('min=1 时组合2 数量 ≥1', qty(r, 'c2') >= 1, `c2=${qty(r, 'c2')}`);
  check('区间场景利用率 totalUsed=6', r.totalUsed === 6, `used=${r.totalUsed}`);
  // max=1：组合1 数量 ≤1
  const r2 = jsSolve({
    houseTypes: [{ id: 't1', name: 'A', quantity: 6 }],
    combinations: [
      { id: 'c1', name: '组合1', items: [{ typeId: 't1', count: 1 }], min: 0, max: 1 },
      { id: 'c2', name: '组合2', items: [{ typeId: 't1', count: 2 }], min: 0, max: 999 },
    ],
    manualInputs: [],
  });
  check('max=1 时组合1 数量 ≤1', qty(r2, 'c1') <= 1, `c1=${qty(r2, 'c1')}`);
  // 固定数量优先于区间：手动固定 5，区间 [1,3] → 结果仍为 5
  const r3 = jsSolve({
    houseTypes: [{ id: 't1', name: 'A', quantity: 10 }],
    combinations: [
      { id: 'c1', name: '组合1', items: [{ typeId: 't1', count: 1 }], min: 1, max: 3 },
    ],
    manualInputs: [{ combinationId: 'c1', quantity: 5 }],
  });
  check('手动固定数量优先于区间', qty(r3, 'c1') === 5, `c1=${qty(r3, 'c1')}`);
}

console.log('⑥ 遍历备选方案（去重 + 标记）');
{
  const resp = jsEnumerate(
    {
      houseTypes: [{ id: 't1', name: 'A', quantity: 6 }],
      combinations: [
        { id: 'c1', name: '组合1', items: [{ typeId: 't1', count: 1 }] },
        { id: 'c2', name: '组合2', items: [{ typeId: 't1', count: 2 }] },
      ],
      manualInputs: []
    },
    10,
  );
  check('至少产出 2 个方案', resp.solutions.length >= 2, `n=${resp.solutions.length}`);
  const sigs = new Set(resp.solutions.map((s) => s.assignments.map((a) => a.quantity).join('|')));
  check('方案互不相同（去重）', sigs.size === resp.solutions.length, `unique=${sigs.size}`);
  check('每个方案 algorithm 含 preview', resp.solutions.every((s) => s.algorithm.includes('preview')));
  check('截断标记为 false（JS 枚举不设截断）', resp.truncated === false, `truncated=${resp.truncated}`);
  check('每个方案利用率不超库存', resp.solutions.every((s) => s.totalRemaining >= 0));
}

console.log('⑦ 大组合数（61 个）不崩溃');
{
  const r = jsSolve({
    houseTypes: [{ id: 't1', name: 'A', quantity: 61 }],
    combinations: Array.from({ length: 61 }, (_, i) => ({
      id: `c${i}`,
      name: `组合${i}`,
      items: [{ typeId: 't1', count: 1 }],
    })),
    manualInputs: []
  });
  check('求解成功（无异常）且用量>0', r.totalUsed > 0, `used=${r.totalUsed}`);
  check('所有数量为非负整数', r.assignments.every((a) => Number.isInteger(a.quantity) && a.quantity >= 0));
}

console.log('⑧ 无自由组合（全手动）algorithm=manual-only');
{
  const r = jsSolve({
    houseTypes: [{ id: 't1', name: 'A', quantity: 5 }],
    combinations: [{ id: 'c1', name: '手动', items: [{ typeId: 't1', count: 1 }] }],
    manualInputs: [{ combinationId: 'c1', quantity: 5 }]
  });
  check('algorithm 为 manual-only', r.algorithm === 'manual-only', r.algorithm);
  check('手动数量 5', qty(r, 'c1') === 5);
}

console.log('⑨ 空输入不崩溃');
{
  const r = jsSolve({ houseTypes: [], combinations: [], manualInputs: [] });
  check('空输入 totalUsed=0', r.totalUsed === 0, `used=${r.totalUsed}`);
}

console.log('⑩ 多户型混合场景（无 NaN / 负库存）');
{
  const r = jsSolve({
    houseTypes: [
      { id: 't1', name: 'A', quantity: 8 },
      { id: 't2', name: 'B', quantity: 12 },
    ],
    combinations: [
      { id: 'c1', name: '混合', items: [{ typeId: 't1', count: 1 }, { typeId: 't2', count: 1 }] },
      { id: 'c2', name: '仅A', items: [{ typeId: 't1', count: 2 }] },
      { id: 'c3', name: '仅B', items: [{ typeId: 't2', count: 3 }] },
    ],
    manualInputs: []
  });
  const allQty = r.assignments.every((a) => Number.isInteger(a.quantity) && a.quantity >= 0);
  const remOk = r.remaining.every((x) => x.remaining >= 0);
  check('数量均为非负整数', allQty);
  check('剩余均非负', remOk);
  check('利用率 ≤ 库存总量 20', r.totalUsed <= 20 && r.totalUsed >= 0, `used=${r.totalUsed}`);
}

console.log(`\n========== 冒烟测试结果：${passed} 通过 / ${failed} 失败 ==========`);
process.exit(failed === 0 ? 0 : 1);
