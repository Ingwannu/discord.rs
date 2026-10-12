impl OAuth2Client {
    pub async fn exchange_code(&self) -> Result<Token, Error> {
        self.send_token_request().await
    }

    async fn send_token_request(&self) -> Result<Token, Error> {
        let response = self
            .http
            .post(format!("{}{}", self.api_base(), RouteId::OAuth2Token.template()))
            .send()
            .await?;
        Ok(response.json().await?)
    }

    #[cfg(test)]
    fn api_base(&self) -> &str {
        &self.api_base
    }
}
