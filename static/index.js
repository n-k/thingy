const { Component, render, createRef } = preact;
const { Router, route } = preactRouter;

class App extends Component {
  constructor(props) {
    super(props);
    this.setState({});
  }

  render(_props, {}) {
    return html`<div class="root">
      <div class="blog">
      </div>
      <${Router}>
        <${Jobs} path="/" default />
        <${Error} type="404" default />
      </${Router}>
    </div>`;
  }
}

class Jobs extends Component {
  constructor(props) {
    super(props);
    this.setState({ jobs: [], loading: true });
  }

  componentDidMount() {
    fetch("/jobs")
      .then((res) => {
        res.json().then((jobs) => {
          this.setState({ jobs });
        });
      })
      .finally(() => {
        this.setState({ loading: false });
      });
  }

  poll = (id) => {
    fetch(`/jobs/${id}/poll`, { method: "POST" }).then((res) => {
      res.json().then((data) => console.log(data));
    });
  }

  renderJob(job) {
    return html`
      <div>
        <div>${job.name}</div>
        <div><a href="javascript:void(0);" onClick=${() => this.poll(job.name)}>Poll now</a></div>
      </div>
    `;
  }

  render() {
    const { loading, jobs } = this.state;
    return html`
      <div>
        ${loading
          ? html`<p>Loading...</p>`
          : jobs.map((j) => this.renderJob(j))}
      </div>
    `;
  }
}

class Error extends Component {
  render() {
    const { type, url } = this.props;
    return html`<section class="error">
      <h2>Error ${type}</h2>
      <p>It looks like we hit a snag.</p>
      <pre>${url}</pre>

      <div>Go to <a href="/">home</a></div>
    </section>`;
  }
}

render(html`<${App} page="All" />`, document.body);
