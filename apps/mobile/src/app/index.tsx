import { Redirect } from "expo-router";
import { session } from "../lib/session";

/** Where to start: the server address, the sign-in, or the library. */
export default function Index() {
  const { server, token } = session();
  if (!server) return <Redirect href="/connect" />;
  if (!token) return <Redirect href="/login" />;
  return <Redirect href="/home" />;
}
