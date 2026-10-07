import { createRoot } from 'react-dom/client'
import App from './App'
import './index.css'
import { BrowserRouter, Routes, Route } from 'react-router-dom'
createRoot(document.getElementById('root')!).render(<BrowserRouter><Routes><Route path="/" element={<App />}/><Route path="/threads/:threadId" element={<App />}/><Route path="/profile" element={<App />}/><Route path="/audit" element={<App />}/><Route path="/tasks" element={<App />}/><Route path="/webhooks" element={<App />}/><Route path="/memories" element={<App />}/></Routes></BrowserRouter>)
