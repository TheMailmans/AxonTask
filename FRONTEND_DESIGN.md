# AxonTask Frontend Design

**Version**: 1.0
**Last Updated**: January 3, 2025
**Framework**: Next.js 14 (App Router) with TypeScript
**Status**: Complete Specification

---

## Technology Stack

### Core
- **Next.js 14**: React framework with App Router
- **TypeScript**: Type safety
- **TailwindCSS**: Utility-first styling
- **Shadcn/ui**: Component library
- **React Hook Form**: Form management
- **Zod**: Schema validation
- **TanStack Query**: Data fetching/caching
- **Zustand**: State management

### Authentication
- **next-auth**: Authentication
- **JWT**: Token storage (httpOnly cookies)

### Real-Time
- **EventSource API**: SSE client for task streaming
- **SWR**: Real-time data synchronization

---

## Application Structure

```
dashboard/
├── app/
│   ├── (auth)/
│   │   ├── login/
│   │   └── register/
│   ├── (dashboard)/
│   │   ├── layout.tsx          # Dashboard layout with sidebar
│   │   ├── page.tsx             # Home/Overview
│   │   ├── tasks/
│   │   │   ├── page.tsx         # Task list
│   │   │   └── [id]/
│   │   │       └── page.tsx     # Task detail + streaming
│   │   ├── api-keys/
│   │   ├── webhooks/
│   │   ├── usage/
│   │   ├── billing/
│   │   └── settings/
│   ├── api/                     # API routes
│   ├── layout.tsx               # Root layout
│   └── providers.tsx
├── components/
│   ├── ui/                      # Shadcn components
│   ├── task-stream-viewer.tsx
│   ├── task-list.tsx
│   ├── api-key-manager.tsx
│   └── usage-chart.tsx
├── lib/
│   ├── api-client.ts
│   ├── auth.ts
│   └── utils.ts
└── types/
    └── index.ts
```

---

## Pages

### 1. Login (`/login`)

**Purpose**: User authentication

**Components**:
- Email input
- Password input
- "Remember me" checkbox
- "Forgot password?" link
- Social OAuth buttons (optional)

**Validation**:
- Email format
- Password minimum 8 characters

**Flow**:
1. Enter credentials
2. Submit → API `/v1/auth/login`
3. Store JWT in httpOnly cookie
4. Redirect to `/tasks`

---

### 2. Register (`/register`)

**Purpose**: New user signup

**Fields**:
- Email
- Password
- Confirm password
- Name
- Tenant name
- Accept ToS checkbox

**Validation**:
- Email unique
- Password strength (min 8 chars, 1 uppercase, 1 number, 1 symbol)
- Passwords match

**Flow**:
1. Fill form
2. Submit → API `/v1/auth/register`
3. Auto-login
4. Redirect to onboarding/dashboard

---

### 3. Dashboard Home (`/`)

**Purpose**: Overview and quick actions

**Sections**:
- **Quick Stats**: 
  - Tasks today/this month
  - Success rate
  - Current usage vs quota
- **Recent Tasks** (last 10)
- **Quick Actions**: "New Task" button
- **Usage Chart**: Task minutes over time

---

### 4. Tasks List (`/tasks`)

**Purpose**: Browse and filter all tasks

**Features**:
- **Filters**:
  - State (all, pending, running, succeeded, failed, canceled)
  - Adapter (all, shell, docker, fly)
  - Date range
- **Sorting**: Created at, started at, duration
- **Search**: By task name
- **Actions**: View, Cancel (if running), Delete

**Table Columns**:
- Name
- Adapter
- State (badge with color)
- Started at
- Duration
- Actions (kebab menu)

**Pagination**: 20 per page

---

### 5. Task Detail (`/tasks/[id]`)

**Purpose**: View task details and live stream

**Layout**: Two-column
- **Left**: Task metadata
  - ID, name, adapter
  - State, timestamps
  - Duration, bytes streamed
  - Created by
- **Right**: Live event stream

**Stream Viewer Component**:
- Auto-scroll to latest
- Pause/resume auto-scroll
- Search/filter events
- Download logs button
- Color-coded by event type:
  - started: blue
  - progress: gray
  - stdout: white
  - stderr: orange
  - success: green
  - error: red

**Auto-Reconnect**: If SSE disconnects, resume from last_seq

---

### 6. API Keys (`/api-keys`)

**Purpose**: Manage API keys

**Features**:
- **List**: Name, prefix, scopes, last used, created at
- **Create**: Modal with name, scopes, expiry
- **Revoke**: Confirm dialog
- **Copy**: One-click copy to clipboard
- **Warning**: "Key shown only once" on creation

**Table Columns**:
- Name
- Key prefix (axon_abc12...)
- Scopes (badges)
- Last used
- Actions (Copy if new, Revoke)

---

### 7. Webhooks (`/webhooks`)

**Purpose**: Manage webhook endpoints

**Features**:
- **List**: URL, events, active status
- **Create**: Modal with URL, events (checkboxes), active toggle
- **Test**: Send test webhook
- **Delete**: Confirm dialog
- **Deliveries**: View recent delivery attempts per webhook

**Webhook Form**:
- URL (validated)
- Events: checkboxes for task.started, task.succeeded, task.failed, task.canceled
- Active: toggle

---

### 8. Usage (`/usage`)

**Purpose**: View usage and quotas

**Charts**:
- **Task Minutes**: Line chart (last 30 days)
- **Tasks Created**: Bar chart by day
- **Success Rate**: Pie chart
- **Streams**: Line chart

**Quota Display**:
- Progress bars with current/limit
- Color-coded (green < 70%, yellow 70-90%, red > 90%)

**Export**: CSV download

---

### 9. Billing (`/billing`)

**Purpose**: Manage subscription

**Sections**:
- **Current Plan**: Name, price, features
- **Usage This Month**: Task-minutes, overage costs
- **Invoices**: List with download links
- **Payment Method**: Update card
- **Upgrade/Downgrade**: Plan comparison table

---

### 10. Settings (`/settings`)

**Tabs**:
- **Profile**: Name, email, avatar, password change
- **Team**: Members, invitations, roles
- **Preferences**: Timezone, notifications
- **API**: Base URL, docs link
- **Danger Zone**: Delete account

---

## Components

### TaskStreamViewer

**Props**:
```tsx
interface TaskStreamViewerProps {
  taskId: string;
  initialSeq?: number;
  autoScroll?: boolean;
}
```

**State**:
- events: Event[]
- isConnected: boolean
- isPaused: boolean
- searchQuery: string

**Features**:
- SSE connection with auto-reconnect
- Resume from cursor on disconnect
- Search/filter events
- Auto-scroll toggle
- Download logs
- Color-coded event types

**Implementation**:
```tsx
const [events, setEvents] = useState<TaskEvent[]>([]);
const [lastSeq, setLastSeq] = useState(initialSeq || 0);
const eventSourceRef = useRef<EventSource | null>(null);

useEffect(() => {
  const connectSSE = () => {
    const es = new EventSource(
      `/v1/mcp/tasks/${taskId}/stream?since_seq=${lastSeq}`,
      { withCredentials: true }
    );
    
    es.onmessage = (e) => {
      const event = JSON.parse(e.data);
      setEvents(prev => [...prev, event]);
      setLastSeq(event.seq);
    };
    
    es.onerror = () => {
      es.close();
      setTimeout(connectSSE, 2000); // Reconnect after 2s
    };
    
    eventSourceRef.current = es;
  };
  
  connectSSE();
  return () => eventSourceRef.current?.close();
}, [taskId, lastSeq]);
```

---

### ApiKeyManager

**Features**:
- Create API key modal
- Copy to clipboard with toast notification
- Revoke confirmation dialog
- Display scopes as badges

---

### UsageChart

**Props**:
```tsx
interface UsageChartProps {
  data: UsageData[];
  metric: 'task_minutes' | 'tasks_created' | 'bytes';
  period: 'day' | 'week' | 'month';
}
```

**Library**: Recharts

---

## Styling

### Color Palette

```css
/* Light mode */
--background: #ffffff;
--foreground: #0a0a0a;
--primary: #2563eb;      /* Blue */
--success: #10b981;      /* Green */
--warning: #f59e0b;      /* Amber */
--error: #ef4444;        /* Red */
--muted: #f1f5f9;

/* Dark mode */
--background: #0a0a0a;
--foreground: #fafafa;
--primary: #3b82f6;
--success: #34d399;
--warning: #fbbf24;
--error: #f87171;
--muted: #1e293b;
```

### Task State Colors

- pending: gray-500
- running: blue-500 (animated pulse)
- succeeded: green-500
- failed: red-500
- canceled: orange-500
- timeout: purple-500

---

## Responsive Design

- **Mobile**: < 768px (single column, hamburger menu)
- **Tablet**: 768-1024px (collapsible sidebar)
- **Desktop**: > 1024px (full sidebar)

---

## Accessibility

- WCAG 2.1 AA compliant
- Keyboard navigation
- Screen reader support
- Focus indicators
- ARIA labels

---

## Performance

- Code splitting per route
- Image optimization (next/image)
- Lazy load components
- SWR for caching
- Debounced search
- Virtualized lists for long task lists

---

## Testing

- **Unit**: Jest + React Testing Library
- **E2E**: Playwright
- **Coverage**: 80%+

---

**Document Version**: 1.0
**Last Updated**: January 3, 2025
**Maintained By**: Tyler Mailman
