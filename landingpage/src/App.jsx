import Nav from './components/Nav.jsx'
import Hero from './components/Hero.jsx'

import Stats from './components/Stats.jsx'
import TechStack from './components/TechStack.jsx'
import Features from './components/Features.jsx'
import Threed from './components/Threed.jsx'
import Architecture from './components/Architecture.jsx'
import CodeSection from './components/CodeSection.jsx'
import ConsolePreview from './components/ConsolePreview.jsx'
import Benchmarks from './components/Benchmarks.jsx'
import Quickstart from './components/Quickstart.jsx'
import CTA from './components/CTA.jsx'
import Footer from './components/Footer.jsx'

export default function App() {
  return (
    <div className="relative min-h-screen bg-ink font-body text-fg">
      <Nav />
      <main>
        <Hero />
        <Stats />
        <TechStack />
        <Features />
        <Threed />
        <Architecture />
        <CodeSection />
        <ConsolePreview />
        <Benchmarks />
        <Quickstart />
        <CTA />
      </main>
      <Footer />
    </div>
  )
}