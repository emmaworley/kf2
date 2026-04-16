import { HashRouter, Routes, Route } from "react-router";
import { HomePage } from "./pages/HomePage.js";

export function App() {
  return (
    <HashRouter>
      <Routes>
        <Route path="/" element={<HomePage />} />
      </Routes>
    </HashRouter>
  );
}
