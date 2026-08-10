import type { Account } from "./oauth";

export function displayAccountName(account: Account): string {
  if (account.display_name && account.display_name.trim().length > 0) {
    return account.display_name.trim();
  }
  if (account.email && account.email.trim().length > 0) {
    return account.email.trim();
  }
  return "Connected account";
}