import type { ReactNode } from "react";
import { createConversationRenderingStressFixture } from "./conversationRenderingStressFixture";

export interface RenderingStressHarnessProps {
  render: (question: ReturnType<typeof createConversationRenderingStressFixture>) => ReactNode;
}

const renderingStressHarnessEnvironment = (
  import.meta as ImportMeta & { env?: { DEV?: boolean; MODE?: string } }
).env;

export function RenderingStressHarness({ render }: RenderingStressHarnessProps): React.ReactElement | null {
  if (!renderingStressHarnessEnvironment?.DEV && renderingStressHarnessEnvironment?.MODE !== "test") return null;

  return (
    <div data-rendering-stress-harness="">
      {render(createConversationRenderingStressFixture())}
    </div>
  );
}
