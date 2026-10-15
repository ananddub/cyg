import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'

function App() {
  return (
    <main className="mx-auto flex min-h-svh max-w-5xl items-center justify-center p-6">
      <Card className="w-full">
        <CardHeader>
          <CardTitle>OpenOxide Control</CardTitle>
        </CardHeader>
        <CardContent className="text-sm text-muted-foreground">
          Dashboard scaffold ready.
        </CardContent>
      </Card>
    </main>
  )
}

export default App
