"use client"

import { useState } from "react"
import { Chat } from "../../components/chat"
import { Settings } from "../../components/settings"

export default function ChatPage() {
  const [selectedModel, setSelectedModel] = useState("gemini-2.0-pro-exp")
  const [apiTokens, setApiTokens] = useState({
    deepseekApiToken: "",
    googleApiToken: ""
  })

  return (
    <main className="relative min-h-screen">
      <Settings 
        onSettingsChange={setApiTokens}
      />
      <Chat 
        selectedModel={selectedModel} 
        onModelChange={setSelectedModel}
        apiTokens={apiTokens}
      />
    </main>
  )
}
