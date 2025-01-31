/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

#[cfg(not(aws_sdk_orchestrator_mode))]
mod middleware_mode_tests {
    use aws_http::retry::AwsResponseRetryClassifier;
    use aws_sdk_kms as kms;
    use aws_smithy_http::body::SdkBody;
    use aws_smithy_http::operation::{self, Parts};
    use aws_smithy_http::response::ParseStrictResponse;
    use aws_smithy_http::result::SdkError;
    use aws_smithy_http::retry::ClassifyRetry;
    use aws_smithy_types::retry::{ErrorKind, RetryKind};
    use bytes::Bytes;
    use kms::operation::create_alias::{CreateAlias, CreateAliasInput};

    async fn create_alias_op() -> Parts<CreateAlias, AwsResponseRetryClassifier> {
        let conf = kms::Config::builder().build();
        let (_, parts) = CreateAliasInput::builder()
            .build()
            .unwrap()
            .make_operation(&conf)
            .await
            .expect("valid request")
            .into_request_response();
        parts
    }

    /// Parse a semi-real response body and assert that the correct retry status is returned
    #[tokio::test]
    async fn errors_are_retryable() {
        let op = create_alias_op().await;
        let http_response = http::Response::builder()
            .status(400)
            .body(Bytes::from_static(
                br#"{ "code": "LimitExceededException" }"#,
            ))
            .unwrap();
        let err = op.response_handler.parse(&http_response).map_err(|e| {
            SdkError::service_error(
                e,
                operation::Response::new(http_response.map(SdkBody::from)),
            )
        });
        let retry_kind = op.retry_classifier.classify_retry(err.as_ref());
        assert_eq!(retry_kind, RetryKind::Error(ErrorKind::ThrottlingError));
    }

    #[tokio::test]
    async fn unmodeled_errors_are_retryable() {
        let op = create_alias_op().await;
        let http_response = http::Response::builder()
            .status(400)
            .body(Bytes::from_static(br#"{ "code": "ThrottlingException" }"#))
            .unwrap();
        let err = op.response_handler.parse(&http_response).map_err(|e| {
            SdkError::service_error(
                e,
                operation::Response::new(http_response.map(SdkBody::from)),
            )
        });
        let retry_kind = op.retry_classifier.classify_retry(err.as_ref());
        assert_eq!(retry_kind, RetryKind::Error(ErrorKind::ThrottlingError));
    }
}

#[cfg(aws_sdk_orchestrator_mode)]
mod orchestrator_mode_tests {
    use aws_credential_types::Credentials;
    use aws_runtime::retries::classifier::AwsErrorCodeClassifier;
    use aws_sdk_kms as kms;
    use aws_smithy_client::test_connection::infallible_connection_fn;
    use aws_smithy_http::result::SdkError;
    use aws_smithy_runtime_api::client::orchestrator::HttpResponse;
    use aws_smithy_runtime_api::client::retries::RetryReason;
    use aws_smithy_types::retry::ErrorKind;
    use bytes::Bytes;
    use kms::operation::create_alias::CreateAliasError;

    async fn make_err(
        response: impl Fn() -> http::Response<Bytes> + Send + Sync + 'static,
    ) -> SdkError<CreateAliasError, HttpResponse> {
        let conn = infallible_connection_fn(move |_| response());
        let conf = kms::Config::builder()
            .http_connector(conn)
            .credentials_provider(Credentials::for_tests())
            .region(kms::config::Region::from_static("us-east-1"))
            .build();
        let client = kms::Client::from_conf(conf);
        client
            .create_alias()
            .send()
            .await
            .expect_err("response was a failure")
    }

    /// Parse a semi-real response body and assert that the correct retry status is returned
    #[tokio::test]
    async fn errors_are_retryable() {
        let err = make_err(|| {
            http::Response::builder()
                .status(400)
                .body(Bytes::from_static(
                    br#"{ "code": "LimitExceededException" }"#,
                ))
                .unwrap()
        })
        .await;

        dbg!(&err);
        let classifier = AwsErrorCodeClassifier;
        let retry_kind = classifier.classify_error(&err);
        assert_eq!(
            Some(RetryReason::Error(ErrorKind::ThrottlingError)),
            retry_kind
        );
    }

    #[tokio::test]
    async fn unmodeled_errors_are_retryable() {
        let err = make_err(|| {
            http::Response::builder()
                .status(400)
                .body(Bytes::from_static(br#"{ "code": "ThrottlingException" }"#))
                .unwrap()
        })
        .await;

        dbg!(&err);
        let classifier = AwsErrorCodeClassifier;
        let retry_kind = classifier.classify_error(&err);
        assert_eq!(
            Some(RetryReason::Error(ErrorKind::ThrottlingError)),
            retry_kind
        );
    }
}
