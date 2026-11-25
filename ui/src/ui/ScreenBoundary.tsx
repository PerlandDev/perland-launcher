import { Component, type ErrorInfo, type ReactNode } from 'react';
import { reportCaught } from '../state/reportErrors';

interface Props {
  /** What to show instead of the broken screen; `reset` draws the screen again. */
  fallback: (error: unknown, reset: () => void) => ReactNode;
  /** The place the screen stands for: when it changes (the player moved), a caught failure is let go. */
  resetKey?: string;
  children: ReactNode;
}

interface State {
  failed: { error: unknown } | null;
  resetKey: string | undefined;
}

/** Catches a screen that breaks while it draws, its lazy chunk failing to load included (a stale file after an
 *  update), so the window shows an error card instead of going blank. Everything outside it keeps working. */
export class ScreenBoundary extends Component<Props, State> {
  state: State = { failed: null, resetKey: this.props.resetKey };

  static getDerivedStateFromError(error: unknown): Partial<State> {
    return { failed: { error } };
  }

  static getDerivedStateFromProps(props: Props, state: State): Partial<State> | null {
    return props.resetKey === state.resetKey ? null : { failed: null, resetKey: props.resetKey };
  }

  componentDidCatch(error: unknown, info: ErrorInfo) {
    console.error('A screen failed to draw', error, info.componentStack);
    // Caught here, it never reaches the window's error event, so it goes to the anonymous reports from here.
    reportCaught(error);
  }

  reset = () => this.setState({ failed: null });

  render() {
    return this.state.failed ? this.props.fallback(this.state.failed.error, this.reset) : this.props.children;
  }
}
