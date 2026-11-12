import "../global.css";
import "../lib/i18n";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Stack } from "expo-router";
import { StatusBar } from "expo-status-bar";
import { useEffect } from "react";
import { AppState } from "react-native";
import { GestureHandlerRootView } from "react-native-gesture-handler";
import { SafeAreaProvider } from "react-native-safe-area-context";
import { Unreachable } from "../lib/api";
import { askNotifications } from "../lib/permissions";
import { BatvisionSync } from "../lib/sync";

const queryClient = new QueryClient({
  defaultOptions: {
    // Offline: don't hammer a server that isn't there.
    queries: {
      retry: (count, error) => !(error instanceof Unreachable) && count < 2,
      staleTime: 30_000,
    },
  },
});

export default function RootLayout() {
  useEffect(() => {
    // Sync when the app opens or comes back to the foreground (D71).
    BatvisionSync.syncNow();
    const sub = AppState.addEventListener("change", (state) => {
      if (state === "active") BatvisionSync.syncNow();
    });
    askNotifications();
    return () => sub.remove();
  }, []);

  return (
    <GestureHandlerRootView style={{ flex: 1 }}>
      <SafeAreaProvider>
        <QueryClientProvider client={queryClient}>
          <StatusBar style="light" />
          <Stack
            screenOptions={{
              headerShown: false,
              contentStyle: { backgroundColor: "#09090c" },
              animation: "fade",
            }}
          />
        </QueryClientProvider>
      </SafeAreaProvider>
    </GestureHandlerRootView>
  );
}
