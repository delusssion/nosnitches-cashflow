import type { CashflowEvent, PaymentType } from '../data/types';
import { fmtDate, fmtMoney, fmtRate } from '../format';
import type { EChartOption } from '../components/EChart';
import { PAYMENT_TYPE_INFO } from './paymentTypes';

// Временная шкала как на схеме эксперта: тело и проценты — флажки над осью,
// субсидия и репрайсинг — под осью; ниже ступенчатая линия остатка долга.
const LANES: { name: string; color: string; y: number; types: PaymentType[] }[] = [
  {
    name: 'Тело',
    color: '#3E5BA8',
    y: 2,
    types: ['principal_payment', 'close_payment', 'prepayment'],
  },
  { name: 'Проценты', color: '#B4541A', y: 1, types: ['interest_payment'] },
  { name: 'Субсидия', color: '#8A4F9E', y: -1, types: ['sub_payment'] },
  { name: 'Репрайсинг', color: '#0E6B57', y: -2, types: ['repricing', 'sub_repricing'] },
];

// Флажок: древко + треугольное полотнище; якорь внизу древка.
const FLAG = 'path://M0,0 L2,0 L2,40 L0,40 Z M2,0 L22,7 L2,14 Z';
const FLAG_DOWN = 'path://M0,0 L2,0 L2,40 L0,40 Z M2,26 L22,33 L2,40 Z';

export function timelineOption(
  events: CashflowEvent[],
  reportDate: string | undefined,
): EChartOption {
  // Общий диапазон для обеих осей, чтобы шкала событий и остаток совпадали по датам.
  const DAY = 86_400_000;
  const times = [
    ...events.map((e) => Date.parse(e.payment_dttm)),
    ...(reportDate ? [Date.parse(reportDate)] : []),
  ];
  const lo = Math.min(...times);
  const hi = Math.max(...times);
  const pad = Math.max(5 * DAY, (hi - lo) * 0.03);
  const range = { min: lo - pad, max: hi + pad };

  const reportLine = (label: boolean) =>
    reportDate
      ? {
          silent: true,
          symbol: 'none',
          lineStyle: { type: 'dashed', color: '#14211D', width: 1.5 },
          label: label
            ? { formatter: `дата расчёта ${fmtDate(reportDate)}`, position: 'end' }
            : { show: false },
          data: [{ xAxis: reportDate }],
        }
      : undefined;

  const lanes = LANES.map((lane, i) => {
    const data = events
      .filter((e) => lane.types.includes(e.payment_type))
      .map((e) => ({ value: [e.payment_dttm, lane.y], event: e }));
    return {
      name: lane.name,
      type: 'scatter',
      xAxisIndex: 0,
      yAxisIndex: 0,
      symbol: lane.y > 0 ? FLAG : FLAG_DOWN,
      symbolSize: [11, 22 + Math.abs(lane.y) * 12],
      symbolOffset: [5, lane.y > 0 ? '-50%' : '50%'],
      itemStyle: { color: lane.color },
      label:
        lane.y === -2
          ? {
              show: true,
              position: 'right',
              offset: [4, 14],
              color: lane.color,
              fontWeight: 600,
              formatter: (p: { data: { event: CashflowEvent } }) => fmtRate(p.data.event.rate),
            }
          : { show: false },
      data,
      markLine: i === 0 ? reportLine(true) : undefined,
    };
  });

  // Остаток: начинаем с остатка до первого события, далее principal_after после каждого.
  const balance: [string, number][] = [];
  const first = events[0];
  if (first?.principal_before != null)
    balance.push([reportDate ?? first.payment_dttm, first.principal_before]);
  for (const e of events)
    if (e.principal_after != null) balance.push([e.payment_dttm, e.principal_after]);

  const tooltipFormatter = (p: {
    seriesName: string;
    data: { event?: CashflowEvent; value: unknown };
  }) => {
    const e = p.data.event;
    if (!e) {
      const [d, v] = p.data.value as [string, number];
      return `${fmtDate(d)}<br/>Остаток: <b>${fmtMoney(v)}</b>`;
    }
    const same = events.filter((x) => x.payment_dttm === e.payment_dttm);
    return [
      `<b>${fmtDate(e.payment_dttm)}</b>`,
      ...same.map(
        (x) =>
          `${PAYMENT_TYPE_INFO[x.payment_type].label}: <b>${
            x.payment_type.includes('repricing') ? fmtRate(x.rate) : fmtMoney(x.amount)
          }</b>`,
      ),
      `Остаток после: ${fmtMoney(same.at(-1)?.principal_after)}`,
    ].join('<br/>');
  };

  return {
    animation: false,
    legend: { top: 0, data: [...LANES.map((l) => l.name), 'Остаток'] },
    tooltip: { trigger: 'item', formatter: tooltipFormatter },
    grid: [
      { left: 90, right: 30, top: 40, height: 210 },
      { left: 90, right: 30, top: 300, height: 120 },
    ],
    xAxis: [
      {
        type: 'time',
        gridIndex: 0,
        ...range,
        axisLine: { onZero: true },
        axisLabel: { show: false },
        splitLine: { show: false },
      },
      { type: 'time', gridIndex: 1, ...range, axisLabel: { formatter: '{dd}.{MM}.{yyyy}' } },
    ],
    yAxis: [
      { type: 'value', gridIndex: 0, min: -3.4, max: 3.4, show: false },
      {
        type: 'value',
        gridIndex: 1,
        name: 'остаток',
        axisLabel: {
          formatter: (v: number) =>
            new Intl.NumberFormat('ru-RU', { notation: 'compact' }).format(v),
        },
      },
    ],
    dataZoom: [
      { type: 'inside', xAxisIndex: [0, 1] },
      { type: 'slider', xAxisIndex: [0, 1], bottom: 6, height: 18 },
    ],
    series: [
      ...lanes,
      {
        name: 'Остаток',
        type: 'line',
        xAxisIndex: 1,
        yAxisIndex: 1,
        step: 'end',
        symbol: 'circle',
        symbolSize: 5,
        itemStyle: { color: '#14211D' },
        areaStyle: { color: 'rgba(14,107,87,0.08)' },
        data: balance.map((value) => ({ value })),
        markLine: reportLine(false),
      },
    ],
  };
}
