import { env } from "cloudflare:workers";
import { drizzle } from "drizzle-orm/d1";
import * as schema from "./schema";

export function getDb() {
  if (!env.DB) {
    throw new Error(
      "The optional D1 example is not configured in the Docker application. Persist product data through the Rust API backed by PostgreSQL."
    );
  }

  return drizzle(env.DB, { schema });
}
