import 'package:composer_protocol/composer_protocol.dart';

const _keep = Object();

/// A server's page reader: the page shown (or a site's micro app), history, follow-agent.
class PageState {
  const PageState({
    this.page,
    this.loading,
    this.error,
    this.back = const [],
    this.forward = const [],
    this.follow = true,
    this.appSite,
  });

  final PageResponse? page;

  /// The URL being fetched.
  final String? loading;
  final String? error;
  final List<String> back;
  final List<String> forward;

  /// Open what the agent opens (in the attached session).
  final bool follow;

  /// Showing this site's micro app instead of the page.
  final String? appSite;

  bool get isOpen => page != null || loading != null || error != null || appSite != null;

  PageState copyWith({
    Object? page = _keep,
    Object? loading = _keep,
    Object? error = _keep,
    List<String>? back,
    List<String>? forward,
    bool? follow,
    Object? appSite = _keep,
  }) =>
      PageState(
        page: identical(page, _keep) ? this.page : page as PageResponse?,
        loading: identical(loading, _keep) ? this.loading : loading as String?,
        error: identical(error, _keep) ? this.error : error as String?,
        back: back ?? this.back,
        forward: forward ?? this.forward,
        follow: follow ?? this.follow,
        appSite: identical(appSite, _keep) ? this.appSite : appSite as String?,
      );
}
