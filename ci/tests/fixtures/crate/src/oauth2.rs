impl OAuth2Client {
    pub async fn exchange_code(&self) -> Result<Token, Error> {
        let response = self
            .http
            .post(format!("{}/oauth2/token", self.api_base()))
            .send()
            .await?;
        Ok(response.json().await?)
    }

    #[cfg(test)]
    fn api_base(&self) -> &str {
        let _ = self.http.get(format!("{}/test/only/oauth", self.api_base));
        &self.api_base
    }
}
