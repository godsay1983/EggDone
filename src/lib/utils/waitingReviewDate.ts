export function reviewDateValue(input: string): string {
  return /^\d{4}\/\d{2}\/\d{2}$/.test(input) ? input.replaceAll('/', '-') : input;
}

export function reviewDateDisplay(value: string): string {
  return /^\d{4}-\d{2}-\d{2}$/.test(value) ? value.replaceAll('-', '/') : value;
}
