export function formatHistoryTime(timestamp: number, now = new Date()) {
  const value = new Date(timestamp);
  if (Number.isNaN(value.getTime())) return "时间未知";
  const startToday = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate(),
  ).getTime();
  const startValue = new Date(
    value.getFullYear(),
    value.getMonth(),
    value.getDate(),
  ).getTime();
  const time = `${String(value.getHours()).padStart(2, "0")}:${String(
    value.getMinutes(),
  ).padStart(2, "0")}`;
  if (startValue === startToday) return `今天 ${time}`;
  const yesterday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1);
  if (startValue === yesterday.getTime()) return `昨天 ${time}`;
  return `${value.getFullYear()}-${String(value.getMonth() + 1).padStart(
    2,
    "0",
  )}-${String(value.getDate()).padStart(2, "0")} ${time}`;
}
