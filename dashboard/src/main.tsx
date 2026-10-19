import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import "./index.css"
import App from "./App.tsx"
import { applyTheme, theme } from "@/lib/theme"

// Before the first paint, so the chosen palette is on <html> the moment the
// first pixel lands — no dark flash in a light session or the other way round.
applyTheme(theme())

// Retrying an unreachable plane is what the 5 s poll and the WS backoff are
// for; react-query's default exponential retries would just delay the failure
// banner by minutes.
const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: 1, refetchOnWindowFocus: false },
  },
})

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </StrictMode>,
)
