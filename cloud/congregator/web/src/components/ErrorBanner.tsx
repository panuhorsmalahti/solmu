type ErrorBannerProps = { message: string }

export function ErrorBanner({ message }: ErrorBannerProps) {
  return message ? <div className="error-banner" role="alert">{message}</div> : null
}
